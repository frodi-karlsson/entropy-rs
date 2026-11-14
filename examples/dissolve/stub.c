// Stands in for dissolve.c in tests. Every 60 frames it prints how far the points have drifted on
// average, as the sum of the drift along x, y and z. It stops after 600 frames, with no window.
// It avoids libm, which the tests don't link on Linux.
#include <stdio.h>
#include "points.h"

static float start_xyz[MAX_POINTS][3], now[MAX_POINTS][3];

double frame(const char *shape) {
    static int frames, count;
    if (frames++ == 0) count = read_points(shape, start_xyz);
    if (frames % 60 == 0) {
        read_points(shape, now);
        double total = 0;
        for (int i = 0; i < count; i++)
            for (int k = 0; k < 3; k++) {
                float d = now[i][k] - start_xyz[i][k];
                total += d < 0 ? -d : d;
            }
        printf("frame %d: %d points, mean drift %.4f\n", frames, count, total / count);
    }
    return frames < 600 ? 1000000 : 0;
}
