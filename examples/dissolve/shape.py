# Writes dissolve.en. Run it again to change the shape.
#
# The word is drawn as strokes on a 4 by 6 grid per letter. Points are scattered along the strokes
# with a little thickness and depth. Each coordinate becomes one character: code point
# CENTRE + 100 * value, so the whole sculpture is one string.
import math
import random

CENTRE = 0x6000
POINTS = 2000

LETTERS = {
    "E": [[(4, 6), (0, 6), (0, 0), (4, 0)], [(0, 3), (3, 3)]],
    "N": [[(0, 0), (0, 6), (4, 0), (4, 6)]],
    "T": [[(0, 6), (4, 6)], [(2, 6), (2, 0)]],
    "R": [[(0, 0), (0, 6), (3, 6), (4, 5), (4, 4), (3, 3), (0, 3)], [(2, 3), (4, 0)]],
    "O": [[(1, 0), (3, 0), (4, 1), (4, 5), (3, 6), (1, 6), (0, 5), (0, 1), (1, 0)]],
    "P": [[(0, 0), (0, 6), (3, 6), (4, 5), (4, 4), (3, 3), (0, 3)]],
    "Y": [[(0, 6), (2, 3), (4, 6)], [(2, 3), (2, 0)]],
}

segments = []
for i, letter in enumerate("ENTROPY"):
    for stroke in LETTERS[letter]:
        for (x0, y0), (x1, y1) in zip(stroke, stroke[1:]):
            segments.append((x0 + 5.5 * i, y0, x1 + 5.5 * i, y1))

scale = 7 / 37
lengths = [math.hypot(x1 - x0, y1 - y0) for x0, y0, x1, y1 in segments]
rng = random.Random(1)
chars = []
for _ in range(POINTS):
    x0, y0, x1, y1 = rng.choices(segments, weights=lengths)[0]
    t = rng.random()
    x = (x0 + (x1 - x0) * t - 18.5) * scale + rng.gauss(0, 0.03)
    y = (y0 + (y1 - y0) * t - 3) * scale + rng.gauss(0, 0.03)
    z = rng.uniform(-0.12, 0.12)
    chars += [chr(CENTRE + round(100 * v)) for v in (x, y, z)]

lines = ["".join(chars[i : i + 48]) for i in range(0, len(chars), 48)]
body = "\n        + ".join(f'"{line}"' for line in lines)

with open("dissolve.en", "w") as f:
    f.write(f"""// seed: 1
// fuel: 50000000
// ffi: stub.c
# The word ENTROPY as a cloud of {POINTS} points, drawn by `frame` in dissolve.c. Each character of
# `shape` is one coordinate: x, y and z for each point in turn. Passing `shape` to `frame` reads
# it, so every coordinate decays a little each frame, and the word slowly dissolves.
#
# shape.py wrote this file.
#
# `frame` returns 1000000 while the window is open and 0 once it closes. The test needs a gap that
# wide, or decay would end the loop on its own.
Program examples dissolve [
    extern frame(string) returns real;
    string shape = {body};
    real open = 1000000;
    while open > 500000 [
        let open = frame(shape);
    ]
]
""")
