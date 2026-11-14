// Reads the points out of the string Entropy passes. Each character is one coordinate, stored as
// its code point: CENTRE + 100 * value.
#define CENTRE 0x6000
#define MAX_POINTS 4096

// Returns the number of points read into xyz.
static int read_points(const char *s, float xyz[][3]) {
    const unsigned char *p = (const unsigned char *)s;
    int n = 0;
    while (*p && n < 3 * MAX_POINTS) {
        unsigned c = *p++;
        int extra = c >= 0xF0 ? 3 : c >= 0xE0 ? 2 : c >= 0xC0 ? 1 : 0;
        if (extra) c &= 0x3F >> extra;
        while (extra-- && (*p & 0xC0) == 0x80) c = c << 6 | (*p++ & 0x3F);
        xyz[n / 3][n % 3] = ((float)c - CENTRE) / 100;
        n++;
    }
    return n / 3;
}
