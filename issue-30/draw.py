import sys
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
from mpl_toolkits.mplot3d.art3d import Poly3DCollection

OUT = Path(sys.argv[1])
OUT.mkdir(parents=True, exist_ok=True)

P = {0: (0, 0), 1: (1, 0), 2: (0, 1), 3: (1, 1), 4: (0.5, 0)}
FRONT = "#2f6fdf"
BACK = "#e0533d"
FILL = ["#9cc0ff", "#ffd27a", "#b5e3a1", "#f3a6c8"]
EDGE = "#334155"


def fig2d():
    fig, ax = plt.subplots(figsize=(2.4, 2.4), dpi=110)
    fig.patch.set_facecolor("white")
    ax.set_xlim(-0.3, 1.3)
    ax.set_ylim(-0.3, 1.3)
    ax.set_aspect("equal")
    ax.axis("off")
    return fig, ax


def arrows(ax, tri, color, shrink=0.72):
    pts = [np.array(P[v], float) for v in tri]
    c = sum(pts) / 3
    pts = [c + shrink * (p - c) for p in pts]
    for a, b in zip(pts, pts[1:] + pts[:1]):
        if np.allclose(a, b):
            continue
        m, d = (a + b) / 2, (b - a)
        ax.annotate("", xy=m + 0.18 * d, xytext=m - 0.18 * d,
                    arrowprops=dict(arrowstyle="-|>", color=color, lw=1.6, mutation_scale=11))


def face(ax, tri, fill, offset=(0, 0), alpha=0.85):
    xy = [(P[v][0] + offset[0], P[v][1] + offset[1]) for v in tri]
    ax.add_patch(plt.Polygon(xy, closed=True, fc=fill, ec=EDGE, lw=1.2, alpha=alpha, zorder=1))


def dots(ax, vs, unused=()):
    for v in vs:
        x, y = P[v]
        hollow = v in unused
        ax.plot(x, y, "o", ms=8, mfc="white" if hollow else EDGE, mec="#94a3b8" if hollow else EDGE, mew=1.5, zorder=5)
        ax.text(x - 0.13 if x < 0.5 else x + 0.13, y - 0.13 if y < 0.5 else y + 0.13, str(v),
                ha="center", va="center", fontsize=11, color="#94a3b8" if hollow else EDGE, weight="bold")


def ring(ax, v, n):
    x, y = P[v]
    for k in range(1, n):
        ax.add_patch(plt.Circle((x, y), 0.05 + 0.05 * k, fill=False, ec=BACK, lw=1.4, zorder=4))


def thick(ax, a, b):
    ax.plot([P[a][0], P[b][0]], [P[a][1], P[b][1]], color=BACK, lw=5, solid_capstyle="round", zorder=3)


def save(fig, name):
    fig.savefig(OUT / f"{name}.png", bbox_inches="tight", pad_inches=0.05, facecolor="white")
    plt.close(fig)


fig, ax = fig2d()
face(ax, (0, 1, 2), FILL[0]); face(ax, (2, 1, 3), FILL[1])
arrows(ax, (0, 1, 2), FRONT); arrows(ax, (2, 1, 3), FRONT)
dots(ax, [0, 1, 2, 3]); save(fig, "clean")

fig, ax = fig2d()
face(ax, (0, 1, 2), FILL[0]); arrows(ax, (0, 1, 2), FRONT)
dots(ax, [0, 1, 2, 3], unused={3}); save(fig, "unused")

fig, ax = fig2d()
face(ax, (0, 1, 2), FILL[0], alpha=0.35); arrows(ax, (0, 1, 2), FRONT)
thick(ax, 0, 1); ring(ax, 0, 2)
dots(ax, [0, 1, 2]); save(fig, "repeated")

fig, ax = fig2d()
face(ax, (0, 1, 2), FILL[0], alpha=0.35); arrows(ax, (0, 1, 2), FRONT)
ring(ax, 3, 3)
dots(ax, [0, 1, 2, 3]); save(fig, "point")

fig, ax = fig2d()
face(ax, (0, 1, 2), FILL[0], alpha=0.35); arrows(ax, (0, 1, 2), FRONT)
thick(ax, 0, 1)
dots(ax, [0, 1, 2, 4]); save(fig, "zero-area")

fig, ax = fig2d()
face(ax, (0, 1, 2), FILL[0], offset=(0.07, 0.07), alpha=0.6)
face(ax, (0, 1, 2), FILL[0]); arrows(ax, (0, 1, 2), FRONT)
dots(ax, [0, 1, 2]); save(fig, "duplicate")

fig, ax = fig2d()
face(ax, (0, 1, 2), FILL[0]); face(ax, (1, 3, 2), FILL[1])
arrows(ax, (0, 1, 2), FRONT); arrows(ax, (1, 3, 2), BACK)
dots(ax, [0, 1, 2, 3]); save(fig, "flipped")

fig, ax = fig2d()
face(ax, (0, 1, 2), FILL[0])
arrows(ax, (0, 1, 2), FRONT, 0.8); arrows(ax, (0, 2, 1), BACK, 0.45)
dots(ax, [0, 1, 2]); save(fig, "two-sided-1")

fig, ax = fig2d()
face(ax, (0, 1, 2), FILL[0]); face(ax, (2, 1, 3), FILL[1])
for t, r in [((0, 1, 2), (0, 2, 1)), ((2, 1, 3), (1, 2, 3))]:
    arrows(ax, t, FRONT, 0.8); arrows(ax, r, BACK, 0.45)
dots(ax, [0, 1, 2, 3]); save(fig, "two-sided-2")

fig, ax = fig2d()
face(ax, (0, 1, 2), FILL[0]); arrows(ax, (0, 1, 2), FRONT)
P[4] = (0.5, -0.03)
ax.plot([0, 0.5, 1], [0, -0.03, 0], color=BACK, lw=5, solid_capstyle="round", zorder=3)
ax.annotate("", xy=(0.75, -0.17), xytext=(0.25, -0.17), arrowprops=dict(arrowstyle="-|>", color=FRONT, lw=1.6))
ax.annotate("", xy=(0.25, -0.26), xytext=(0.75, -0.26), arrowprops=dict(arrowstyle="-|>", color=BACK, lw=1.6))
dots(ax, [0, 1, 2]); save(fig, "sliver")
P[4] = (0.5, 0)

fig = plt.figure(figsize=(2.6, 2.4), dpi=110)
fig.patch.set_facecolor("white")
ax = fig.add_subplot(projection="3d")
ax.computed_zorder = False
V = {0: (0, 0, 0), 1: (1, 0, 0), 2: (0.5, 0.9, -0.35), 3: (0.5, -0.9, -0.35), 5: (0.5, 0, 0.95)}
for i, tri in enumerate([(0, 1, 2), (1, 0, 3), (1, 0, 5)]):
    ax.add_collection3d(Poly3DCollection([[V[v] for v in tri]], fc=FILL[i], ec=EDGE, alpha=0.85))
ax.plot([0, 1], [0, 0], [0, 0], color=BACK, lw=5, zorder=10)
for v, (x, y, z) in V.items():
    ax.text(x, y, z + 0.08, str(v), fontsize=10, weight="bold", color=EDGE, zorder=11)
ax.set_xlim(0, 1); ax.set_ylim(-0.9, 0.9); ax.set_zlim(-0.4, 0.95)
ax.view_init(elev=12, azim=-12)
ax.set_axis_off()
save(fig, "fin")

fig, axes = plt.subplots(1, 3, figsize=(5.4, 1.9), dpi=110)
fig.patch.set_facecolor("white")
pts = [(0.1, 0.2), (0.9, 0.3), (0.45, 0.9)]
for ax, labels, name in zip(axes, [(0, 1, 2), (0, 2, 1), (0, 1, 2)], ["tile 1", "tile 2", "tile 3"]):
    ax.set_xlim(-0.1, 1.1); ax.set_ylim(-0.05, 1.1); ax.set_aspect("equal"); ax.axis("off")
    for (x, y), l, col in zip(pts, labels, ["#64748b", FRONT, BACK]):
        ax.plot(x, y, "o", ms=12, color=col)
        ax.text(x, y + 0.13, str(l), ha="center", fontsize=12, weight="bold", color=col)
    ax.set_title(name, fontsize=10, color=EDGE)
save(fig, "fuzz-flip")
