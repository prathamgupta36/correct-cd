"""
Seed eval dataset. Synthetic but realistic. Marked SEED because the real
tuning signal comes from replaying actual logged history later — this set
exists to (a) sanity-check both configs, (b) stress the 3 knobs so the
configs disagree, (c) catch regressions as we iterate.

DIRS: {path: [visit ages in days]}  -> frecency is derived per-config from H.
QUERIES: (fragment, cwd, intended_path_or_None, tag)
  intended = None  means "should stay SILENT and fall through to native".
"""

HOME = "/home/u"

DIRS = {
    f"{HOME}/Downloads":                 [0.1, 0.2, 0.3, 0.5, 1, 1, 2, 3, 3, 4, 5, 8, 10],
    f"{HOME}/Documents":                 [2, 5, 9, 15, 20],
    f"{HOME}/Desktop":                   [30, 45],
    f"{HOME}/Pictures":                  [12, 40],
    f"{HOME}/Music":                     [60],
    f"{HOME}/Videos":                    [90],
    f"{HOME}/dev/web-node":              [0.2, 0.4, 1, 1, 2, 2, 3, 3, 3, 4, 5, 6, 7],
    f"{HOME}/dev/web-node/src":          [0.3, 0.5, 1, 2, 2, 3, 4],
    f"{HOME}/dev/web-node/node_modules": [5],
    f"{HOME}/dev/ccd":                   [0.1, 0.2, 0.5, 1, 1, 2],
    f"{HOME}/dev/ccd/src":               [0.2, 0.5, 1],
    f"{HOME}/dev/data-pipeline":         [22, 23, 24, 25, 25, 26, 28, 30],  # heavy but OLD -> H knob
    f"{HOME}/dev/data-pipeline/scripts": [24, 26],
    f"{HOME}/projects":                  [3, 10, 20],
    f"{HOME}/projects/downtown-notes":   [4, 11, 18],
    f"{HOME}/projects/photography":      [15, 35],
    f"{HOME}/work/reports":              [7, 14, 21, 28],
    f"{HOME}/work/reports/2026-q2":      [7, 8, 14],
    f"{HOME}/.config":                   [16, 50],

    # --- H-conflict pair: heavy-but-abandoned vs light-but-current ---
    f"{HOME}/dev/report-service":        [18, 19, 20, 20, 21, 22, 22, 23,
                                          24, 25, 25, 26, 27, 28, 29, 30],  # sprint ended ~3wk ago
    f"{HOME}/dev/report-ui":             [0.5, 1, 1, 2, 3, 4, 5, 6],         # sustained THIS week
    # --- frequency-should-win pair: long-term main vs recent-accidental ---
    f"{HOME}/dev/kernel":                [0.5, 2, 4, 7, 10, 12, 14, 16, 18, 20,
                                          22, 25, 28, 30, 33, 36, 40, 45, 50, 55],
    f"{HOME}/tmp/kernel-panic-logs":     [1, 2],
    # --- third 'src' for a 3-way basename tie ---
    f"{HOME}/work/dashboard/src":        [8, 16],
}

QUERIES = [
    # --- clean prefixes (frecency decides among matches) ---
    ("down",        HOME, f"{HOME}/Downloads",                "prefix"),
    ("doc",         HOME, f"{HOME}/Documents",                "prefix"),
    ("desk",        HOME, f"{HOME}/Desktop",                  "prefix"),
    # NOTE: bare "rep" dropped as a labeled case — after adding report-service/
    # report-ui it's genuinely 3-way ambiguous, so it has no single ground
    # truth; correct behavior is "most-frecent first, rest via Tab-cycle".

    # --- abbreviations (dropped letters -> subsequence) ---
    ("dwn",         HOME, f"{HOME}/Downloads",                "abbrev"),
    ("dwnld",       HOME, f"{HOME}/Downloads",                "abbrev"),
    ("docs",        HOME, f"{HOME}/Documents",                "abbrev"),
    ("wn",          HOME, f"{HOME}/dev/web-node",             "abbrev"),
    ("ccd",         HOME, f"{HOME}/dev/ccd",                  "abbrev"),
    ("dp",          HOME, f"{HOME}/dev/data-pipeline",        "abbrev"),
    ("dwntwn",      HOME, f"{HOME}/projects/downtown-notes",  "abbrev"),
    ("q2",          HOME, f"{HOME}/work/reports/2026-q2",     "abbrev"),

    # --- typos (wrong / swapped / extra / missing char -> edit distance) ---
    ("Doanloads",   HOME, f"{HOME}/Downloads",                "typo-sub"),
    ("Donwloads",   HOME, f"{HOME}/Downloads",                "typo-transpose"),
    ("Dowloads",    HOME, f"{HOME}/Downloads",                "typo-missing"),
    ("Downloadss",  HOME, f"{HOME}/Downloads",                "typo-extra"),
    ("documnets",   HOME, f"{HOME}/Documents",                "typo-transpose"),
    ("desktp",      HOME, f"{HOME}/Desktop",                  "typo-missing"),
    ("photogrpahy", HOME, f"{HOME}/projects/photography",     "typo-transpose"),

    # --- recency / H knob (data-pipeline heavy but ~24d old) ---
    ("data",        HOME, f"{HOME}/dev/data-pipeline",        "recency-H"),
    ("pipeline",    HOME, f"{HOME}/dev/data-pipeline",        "recency-H"),

    # --- MIN_MATCH knob: weak-but-CORRECT (favors lower threshold) ---
    ("dcmt",        HOME, f"{HOME}/Documents",                "weak-correct"),

    # --- MIN_MATCH knob: weak + should stay SILENT (favors higher threshold) ---
    ("musx",        HOME, None,                               "weak-silent"),
    ("vdz",         HOME, None,                               "weak-silent"),

    # --- genuine no-match: must stay silent under any config ---
    ("xyz",         HOME, None,                               "no-match"),
    ("qqzz",        HOME, None,                               "no-match"),

    # --- multi-segment ---
    ("web/src",     HOME, f"{HOME}/dev/web-node/src",         "multiseg"),
    ("ccd/src",     HOME, f"{HOME}/dev/ccd/src",              "multiseg"),

    # --- ambiguous same-basename (two 'src' -> frecency tiebreak) ---
    ("src",         HOME, f"{HOME}/dev/web-node/src",         "ambiguous"),

    # --- context/child boost: cwd inside ccd should flip 'src' ---
    ("src",         f"{HOME}/dev/ccd", f"{HOME}/dev/ccd/src", "child-ctx"),

    # --- H conflict: you moved to report-ui this week -> short H should win ---
    ("report",      HOME, f"{HOME}/dev/report-ui",              "H-recency-switch"),
    # --- frequency should win: kernel is the long-term main project ---
    ("kernel",      HOME, f"{HOME}/dev/kernel",                 "H-frequency-main"),

    # --- borderline threshold (measured, labeled by intent) ---
    ("videoss",     HOME, f"{HOME}/Videos",                     "weak-correct"),
    ("dokuments",   HOME, f"{HOME}/Documents",                  "weak-correct"),
    ("confg",       HOME, f"{HOME}/.config",                    "weak-correct"),
    ("picx",        HOME, None,                                 "weak-silent"),
    ("mzk",         HOME, None,                                 "weak-silent"),

    # --- 3-way basename tie (frecency picks the most-used src) ---
    ("src2",        HOME, f"{HOME}/dev/web-node/src",           "3way-tie"),  # alias, see note
]

# NOTE: "src2" is a label-only duplicate of "src" so the 3-way tie (now that a
# third src exists in work/dashboard) is scored as its own line item; the
# fragment actually sent is "src".
QUERIES = [(("src" if f == "src2" else f), c, i, t) for (f, c, i, t) in QUERIES]
