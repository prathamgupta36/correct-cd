"""
A/B eval harness for ccd ranking configs.

    python3 eval.py          # compare configs A and B + show disagreements
    python3 eval.py --grid   # sweep params, print best by objective

Metrics per config (over the labeled seed set):
  target queries (intended != None):
    hit      top1 == intended               (good)
    wrong    suggested but top1 != intended  (WORST — sends you to wrong dir)
    miss     stayed silent though a target existed (safe-ish, just unhelpful)
  silent queries (intended == None):
    ok_silent   stayed silent      (good)
    false_sug   suggested anyway   (bad — noise / wrong jump)

Objective (higher=better), reflecting "never send me somewhere wrong":
    obj = hit + ok_silent - 2*wrong - 2*false_sug - 0.5*miss   (as % of max)
"""

import sys
from ranker import Config, rank, top1
from dataset import DIRS, QUERIES

# ------- the two configs under test -------
CONFIG_A = Config(name="B (yours, H=30)",     alpha=1.5, beta=1.0, H_days=30, K=4, min_match=0.70)
CONFIG_B = Config(name="FINAL (evidence H=14)", alpha=1.5, beta=1.0, H_days=14, K=4, min_match=0.70)
# alpha probe: does 2.0 beat 1.5 at the corrected gate/half-life?
CONFIG_ALPHA = Config(name="alpha=2.0 probe",  alpha=2.0, beta=1.0, H_days=14, K=4, min_match=0.70)


def evaluate(cfg, queries):
    rows = []
    hit = wrong = miss = ok_silent = false_sug = 0
    rr_sum = 0.0
    n_target = 0
    for frag, cwd, intended, tag in queries:
        ranked = rank(frag, cwd, DIRS, cfg)
        pred = ranked[0]["path"] if ranked else None
        if intended is None:
            outcome = "ok_silent" if pred is None else "FALSE_SUG"
            if pred is None:
                ok_silent += 1
            else:
                false_sug += 1
        else:
            n_target += 1
            if pred is None:
                outcome = "miss"
                miss += 1
            elif pred == intended:
                outcome = "hit"
                hit += 1
            else:
                outcome = "WRONG"
                wrong += 1
            # reciprocal rank of intended in the ranked list
            for i, r in enumerate(ranked):
                if r["path"] == intended:
                    rr_sum += 1.0 / (i + 1)
                    break
        rows.append((frag, tag, intended, pred, outcome))

    n_silent = len(queries) - n_target
    max_obj = n_target + n_silent
    obj = hit + ok_silent - 2 * wrong - 2 * false_sug - 0.5 * miss
    return {
        "hit": hit, "wrong": wrong, "miss": miss,
        "ok_silent": ok_silent, "false_sug": false_sug,
        "n_target": n_target, "n_silent": n_silent,
        "acc": hit / n_target if n_target else 0,
        "precision": hit / (hit + wrong) if (hit + wrong) else 1.0,
        "specificity": ok_silent / n_silent if n_silent else 1.0,
        "mrr": rr_sum / n_target if n_target else 0,
        "obj_pct": 100 * obj / max_obj,
        "rows": rows,
    }


def summarize(cfg, m):
    print(f"\n=== {cfg.name} ===")
    print(f"  alpha={cfg.alpha} beta={cfg.beta} H={cfg.H_days}d "
          f"min_match={cfg.min_match}")
    print(f"  top-1 accuracy : {m['acc']*100:5.1f}%   "
          f"({m['hit']}/{m['n_target']} targets)")
    print(f"  precision      : {m['precision']*100:5.1f}%   "
          f"(when it suggests, how often right)")
    print(f"  specificity    : {m['specificity']*100:5.1f}%   "
          f"(stays silent when it should: {m['ok_silent']}/{m['n_silent']})")
    print(f"  MRR            : {m['mrr']:.3f}")
    print(f"  breakdown      : hit={m['hit']} WRONG={m['wrong']} "
          f"miss={m['miss']} false_suggest={m['false_sug']}")
    print(f"  OBJECTIVE      : {m['obj_pct']:5.1f}%")


def show_disagreements(ma, mb):
    print("\n=== WHERE A AND B DISAGREE (the iteration signal) ===")
    n = 0
    for (fa, tag, intended, pa, oa), (_, _, _, pb, ob) in zip(ma["rows"], mb["rows"]):
        if pa == pb and oa == ob:
            continue
        n += 1
        want = intended.split("/")[-1] if intended else "SILENT"
        sa = pa.split("/")[-1] if pa else "silent"
        sb = pb.split("/")[-1] if pb else "silent"
        flag_a = "" if oa in ("hit", "ok_silent") else " <-bad"
        flag_b = "" if ob in ("hit", "ok_silent") else " <-bad"
        print(f"  {fa:<12} [{tag:<14}] want={want:<16} "
              f"A->{sa:<14}{flag_a:<6}  B->{sb:<14}{flag_b}")
    if n == 0:
        print("  (none — configs behave identically on this set)")
    print(f"\n  {n} disagreement(s).")


def grid_search():
    print("Grid search (objective %, higher=better)")
    print("NOTE: overfits this synthetic seed set — directional only until "
          "we replay real history.\n")
    best = []
    for alpha in (1.0, 1.25, 1.5, 1.75, 2.0, 2.5):
        for beta in (0.5, 0.75, 1.0, 1.25):
            for H in (7, 14, 30, 60):
                for mm in (0.50, 0.55, 0.60, 0.65, 0.70, 0.75):
                    cfg = Config("grid", alpha, beta, H, 4, mm)
                    m = evaluate(cfg, QUERIES)
                    best.append((m["obj_pct"], m["acc"], m["specificity"],
                                 alpha, beta, H, mm, m["wrong"], m["false_sug"]))
    best.sort(reverse=True)
    print(f"{'obj%':>6} {'acc%':>6} {'spec%':>6}  "
          f"{'a':>4} {'b':>4} {'H':>3} {'mm':>4}  wrong false")
    for obj, acc, spec, a, b, H, mm, w, fs in best[:12]:
        print(f"{obj:6.1f} {acc*100:6.1f} {spec*100:6.1f}  "
              f"{a:4} {b:4} {H:3} {mm:4}  {w:5} {fs:5}")


def sweep_min_match():
    """Hold B's other params; sweep the gate. Shows the tradeoff between
    keeping correct weak matches vs leaking false suggestions."""
    print("\nMIN_MATCH sweep (alpha=1.5 beta=1.0 H=30):")
    print(f"{'min_match':>9}  {'hit':>4} {'WRONG':>5} {'miss':>4} "
          f"{'false':>5}  {'obj%':>6}")
    for mm in (0.55, 0.60, 0.65, 0.68, 0.70, 0.72, 0.75, 0.80):
        cfg = Config("s", 1.5, 1.0, 30, 4, mm)
        m = evaluate(cfg, QUERIES)
        print(f"{mm:9.2f}  {m['hit']:4} {m['wrong']:5} {m['miss']:4} "
              f"{m['false_sug']:5}  {m['obj_pct']:6.1f}")


def sweep_H():
    """Hold B's other params; sweep the half-life. Now that H-conflict cases
    exist, this should finally discriminate."""
    print("\nH sweep (alpha=1.5 beta=1.0 min_match=0.70):")
    print(f"{'H_days':>6}  {'hit':>4} {'WRONG':>5} {'miss':>4} "
          f"{'false':>5}  {'obj%':>6}   report->  kernel->")
    for H in (7, 10, 14, 21, 30, 45, 60):
        cfg = Config("s", 1.5, 1.0, H, 4, 0.70)
        m = evaluate(cfg, QUERIES)
        rep = top1("report", "/home/u", DIRS, cfg)
        ker = top1("kernel", "/home/u", DIRS, cfg)
        print(f"{H:6}  {m['hit']:4} {m['wrong']:5} {m['miss']:4} "
              f"{m['false_sug']:5}  {m['obj_pct']:6.1f}   "
              f"{(rep or 'silent').split('/')[-1]:<10} "
              f"{(ker or 'silent').split('/')[-1]}")


if __name__ == "__main__":
    if "--grid" in sys.argv:
        grid_search()
    elif "--sweep" in sys.argv:
        sweep_min_match()
        sweep_H()
    else:
        ma = evaluate(CONFIG_A, QUERIES)
        mb = evaluate(CONFIG_B, QUERIES)
        mp = evaluate(CONFIG_ALPHA, QUERIES)
        summarize(CONFIG_A, ma)
        summarize(CONFIG_B, mb)
        summarize(CONFIG_ALPHA, mp)
        print("\n--- B(H=30) vs FINAL(H=14) ---")
        show_disagreements(ma, mb)
        print("\n--- FINAL(alpha=1.5) vs alpha=2.0 probe ---")
        show_disagreements(mb, mp)
