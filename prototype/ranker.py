"""
ccd ranker — reference implementation of the locked ranking spec.

This is the Python prototype used to A/B-test parameters. Once numbers are
frozen here, the exact same logic + constants get ported to the Rust core.

Two error models, unified:
  - abbreviation ("Dwn" -> "Downloads")  -> subsequence score
  - typo        ("Doanloads" -> "...")   -> Damerau/OSA edit distance
  fuzzy = max(subsequence, edit)

final = fuzzy**alpha * frecency_norm**beta   (gated by min_match)
"""

from dataclasses import dataclass


@dataclass
class Config:
    name: str
    alpha: float        # fuzzy exponent  (match-correctness weight)
    beta: float         # frecency exponent (habit weight)
    H_days: float       # frecency half-life
    K: float            # frecency saturation constant
    min_match: float    # hard gate: suggest only if fuzzy >= this
    gamma_gap: float = 0.7
    gamma_pos: float = 0.3
    boundary_base: float = 0.1    # floor bonus if any matched char is a boundary
    acronym_scale: float = 0.25   # extra bonus scaling with # of word-initials
    child_boost: float = 1.75
    child_margin: float = 0.05    # child boost only applies within this of best fuzzy


# ---------- string matching ----------

def osa_distance(a: str, b: str) -> int:
    """Optimal String Alignment (restricted Damerau-Levenshtein).
    Handles insert/delete/substitute + adjacent transposition."""
    la, lb = len(a), len(b)
    if la == 0:
        return lb
    if lb == 0:
        return la
    prev2 = [0] * (lb + 1)
    prev = list(range(lb + 1))
    for i in range(1, la + 1):
        cur = [i] + [0] * lb
        for j in range(1, lb + 1):
            cost = 0 if a[i - 1] == b[j - 1] else 1
            cur[j] = min(
                prev[j] + 1,          # deletion
                cur[j - 1] + 1,       # insertion
                prev[j - 1] + cost,   # substitution
            )
            if (i > 1 and j > 1
                    and a[i - 1] == b[j - 2]
                    and a[i - 2] == b[j - 1]):
                cur[j] = min(cur[j], prev2[j - 2] + 1)  # transposition
        prev2, prev = prev, cur
    return prev[lb]


def subseq_score(q: str, base: str, cfg: Config) -> float:
    """fzf-style subsequence score in [0,1]. 0 if q is not a subsequence."""
    ql, bl = q.lower(), base.lower()
    if not ql:
        return 0.0
    positions = []
    j = 0
    for ch in ql:
        found = -1
        for k in range(j, len(bl)):
            if bl[k] == ch:
                found = k
                break
        if found == -1:
            return 0.0
        positions.append(found)
        j = found + 1
    m = len(ql)
    first, last = positions[0], positions[-1]
    span = last - first + 1
    n = max(len(bl), 1)
    score = 1.0 - cfg.gamma_gap * (span - m) / n - cfg.gamma_pos * first / n
    # acronym/initialism bonus: scales with how many matched chars land on a
    # word boundary. "wn" -> [w]eb-[n]ode (2/2) beats a compact interior match.
    at_boundary = 0
    for p in positions:
        if (p == 0
                or base[p - 1] in "-_. /"
                or (base[p].isupper() and base[p - 1].islower())):
            at_boundary += 1
    if at_boundary:
        score += cfg.boundary_base + cfg.acronym_scale * (at_boundary / m)
    return max(0.0, min(1.0, score))


def edit_score(q: str, base: str) -> float:
    d = osa_distance(q.lower(), base.lower())
    return 1.0 - d / max(len(q), len(base), 1)


def fuzzy_score(q: str, base: str, cfg: Config) -> float:
    return max(subseq_score(q, base, cfg), edit_score(q, base))


# ---------- frecency ----------

def frecency_eff(visits_days_ago, H_days: float) -> float:
    """Sum of half-life-decayed visit weights == the incremental counter's
    effective value at query time."""
    return sum(2.0 ** (-d / H_days) for d in visits_days_ago)


def frecency_norm(f_eff: float, K: float) -> float:
    return f_eff / (f_eff + K)


# ---------- ranking ----------

def _basename(path: str) -> str:
    return [c for c in path.split("/") if c][-1]


def _components(path: str):
    return [c for c in path.split("/") if c]


def _multiseg_fuzzy(qsegs, path, cfg: Config):
    """For 'proj/src'-style fragments: match segments as an ordered
    subsequence over path components. Returns the last segment's score,
    or None if any earlier segment can't clear the gate."""
    comps = _components(path)
    ci = 0
    last_score = 0.0
    for qi, qs in enumerate(qsegs):
        best_sc, best_k = 0.0, -1
        for k in range(ci, len(comps)):
            sc = fuzzy_score(qs, comps[k], cfg)
            if sc > best_sc:
                best_sc, best_k = sc, k
        is_last = qi == len(qsegs) - 1
        # earlier segments must clear the gate; last is gated by caller
        if not is_last and best_sc < cfg.min_match:
            return None
        if best_k == -1:
            return None
        ci = best_k + 1
        last_score = best_sc
    return last_score


def rank(fragment: str, cwd: str, dirs: dict, cfg: Config):
    """dirs: {path: [visit_days_ago,...]}. Returns gate-passing candidates,
    best first, as list of dicts."""
    qsegs = [s for s in fragment.split("/") if s]
    multiseg = len(qsegs) > 1
    # pass 1: gather gate-passing candidates with their fuzzy scores
    cands = []
    for path, visits in dirs.items():
        if multiseg:
            fz = _multiseg_fuzzy(qsegs, path, cfg)
            if fz is None:
                continue
        else:
            fz = fuzzy_score(fragment, _basename(path), cfg)
        if fz < cfg.min_match:
            continue
        cands.append((path, visits, fz))
    if not cands:
        return []
    max_fuzzy = max(fz for _, _, fz in cands)
    # pass 2: score. child boost only breaks ties among near-best matches,
    # so a weaker match can never be boosted past a stronger one.
    out = []
    for path, visits, fz in cands:
        f_eff = frecency_eff(visits, cfg.H_days)
        fnorm = frecency_norm(f_eff, cfg.K)
        final = (fz ** cfg.alpha) * (fnorm ** cfg.beta)
        is_child = (cwd and path.startswith(cwd + "/")
                    and "/" not in path[len(cwd) + 1:])
        if is_child and fz >= max_fuzzy - cfg.child_margin:
            final *= cfg.child_boost
        out.append({
            "path": path, "fuzzy": fz, "f_eff": f_eff,
            "fnorm": fnorm, "final": final,
        })
    # deterministic tie-break: final, then f_eff, then shorter path, then lex
    out.sort(key=lambda r: (-r["final"], -r["f_eff"], len(r["path"]), r["path"]))
    return out


def top1(fragment, cwd, dirs, cfg):
    r = rank(fragment, cwd, dirs, cfg)
    return r[0]["path"] if r else None
