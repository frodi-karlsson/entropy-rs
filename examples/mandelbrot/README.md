# Mandelbrot

`mandelbrot.en` computes the Mandelbrot set and hands each pixel to `plot` in `plot.c`, which
writes a PPM image. At the low rate below the shape survives, though drift bends it.

Run it in memory, with `plot.c` built as a shared library:

```sh
cc -shared -fPIC -o libplot.so plot.c
entropy run mandelbrot.en --mutation-rate 0.01 --dylib ./libplot.so > mandelbrot.ppm
```

Or build an executable, with `plot.c` as a static library:

```sh
cc -c plot.c && ar rcs libplot.a plot.o
entropy build mandelbrot.en --mutation-rate 0.01 -L . -l plot
./mandelbrot > mandelbrot.ppm
```

`ascii.en` draws the same set as text, with no C at all.
