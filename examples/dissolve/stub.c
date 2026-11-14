// Stands in for dissolve.c in tests. Every 60 frames it prints how far the points have drifted on
// average. It stops after 600 frames, with no window.
#include <math.h>
#include <stdio.h>
#include "points.h"

static float start_xyz[MAX_POINTS][3], now[MAX_POINTS][3];

double frame(const char *shape) {
    static int frames, count;
    if (frames++ == 0) count = read_points(shape, start_xyz);
    if (frames % 60 == 0) {
        read_points(shape, now);
        double total = 0;
        for (int i = 0; i < count; i++) {
            float dx = now[i][0] - start_xyz[i][0], dy = now[i][1] - start_xyz[i][1];
            float dz = now[i][2] - start_xyz[i][2];
            total += sqrt(dx * dx + dy * dy + dz * dz);
        }
        printf("frame %d: %d points, mean drift %.4f\n", frames, count, total / count);
    }
    return frames < 600 ? 1000000 : 0;
}
