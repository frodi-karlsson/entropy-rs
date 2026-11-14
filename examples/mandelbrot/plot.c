/* Collects Mandelbrot pixels from Entropy and writes them as a text PPM.
 *
 * The coordinates Entropy passes drift like every other value, so each one is rounded to the
 * nearest pixel, and anything off the grid is ignored. A pixel never plotted stays black. It needs
 * no libm, so it links the same way on macOS and Linux. */
#include <stdio.h>

#define WIDTH 80
#define HEIGHT 40
#define LIMIT 50

static double pixels[HEIGHT][WIDTH];
static int plotted[HEIGHT][WIDTH];

/* Exactly the values that round to a pixel inside the grid. NaN fails every comparison. */
static int on_grid(double v, int size) { return v > -0.5 && v < size - 0.5; }

void plot(double x, double y, double iter) {
    if (!on_grid(x, WIDTH) || !on_grid(y, HEIGHT)) {
        return;
    }
    /* v is above -0.5 here, so adding 0.5 and truncating rounds to nearest, without libm. */
    int col = (int)(x + 0.5), row = (int)(y + 0.5);
    pixels[row][col] = iter;
    plotted[row][col] = 1;
}

void finish(void) {
    printf("P3\n%d %d\n255\n", WIDTH, HEIGHT);
    for (int row = 0; row < HEIGHT; row++) {
        for (int col = 0; col < WIDTH; col++) {
            double iter = pixels[row][col];
            /* Inside the set, unplotted, or drifted out of range: black. */
            if (!plotted[row][col] || !(iter >= 0 && iter < LIMIT)) {
                printf("0 0 0\n");
                continue;
            }
            double t = iter / LIMIT;
            int r = (int)(9 * (1 - t) * t * t * t * 255);
            int g = (int)(15 * (1 - t) * (1 - t) * t * t * 255);
            int b = (int)(8.5 * (1 - t) * (1 - t) * (1 - t) * t * 255);
            printf("%d %d %d\n", r, g, b);
        }
    }
}
