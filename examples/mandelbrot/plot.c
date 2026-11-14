/* Collects Mandelbrot pixels from Entropy and writes them as a text PPM.
 *
 * The coordinates Entropy passes drift like every other value, so each one is rounded to the
 * nearest pixel, and anything off the grid is ignored. A pixel never plotted stays black. */
#include <math.h>
#include <stdio.h>

#define WIDTH 80
#define HEIGHT 40
#define LIMIT 50

static double pixels[HEIGHT][WIDTH];
static int plotted[HEIGHT][WIDTH];

void plot(double x, double y, double iter) {
    double col = round(x), row = round(y);
    if (!(col >= 0 && col < WIDTH && row >= 0 && row < HEIGHT)) {
        return;
    }
    pixels[(int)row][(int)col] = iter;
    plotted[(int)row][(int)col] = 1;
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
