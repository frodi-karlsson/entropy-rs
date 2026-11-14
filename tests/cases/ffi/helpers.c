/* C functions for the FFI golden cases. Each prints through stdio, to test output ordering. */
#include <stdint.h>
#include <stdio.h>

double add3(double a, double b, double c) { return a + b + c; }
double half(double x) { return x / 2; }
int32_t next_char(int32_t c) { return c + 1; }
void show(const char *s) { printf("[%s]", s); }
void show2(const char *a, const char *b) { printf("[%s|%s]", a, b); }
void show_char(int32_t c) { printf("<%d>", c); }
void show_real(double x) { printf("<%g>", x); }
void newline(void) { printf("\n"); }
