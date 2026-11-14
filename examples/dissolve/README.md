# Dissolve

The word ENTROPY as a cloud of glowing points that slowly falls apart, in a window, until you close
it.

Entropy has no arrays, but a string will do. `dissolve.en` keeps the whole sculpture in one string:
each character is one coordinate, stored as its code point. Every frame it passes the string to
`frame` in `dissolve.c`, which draws it with [raylib](https://www.raylib.com/). Passing the string
reads it, and every read decays every character. So each point drifts a little every frame, and the
word blurs, then scatters into a cloud. A point turns from blue to red as it wanders.

At the default rate the word is readable for a few minutes. At `--mutation-rate 5` it lasts a few
seconds.

You need raylib (`brew install raylib`, or `libraylib-dev` on Debian and Ubuntu). From the
repository root on macOS:

```sh
cc -shared -fPIC -o /tmp/libdissolve.so examples/dissolve/dissolve.c -I/opt/homebrew/include -L/opt/homebrew/lib -lraylib
entropy run examples/dissolve/dissolve.en --dylib /tmp/libdissolve.so
```

Add `--seed N` to watch the same run again.

`shape.py` wrote `dissolve.en`. Change the word or the number of points there. The tests use
`stub.c` in place of `dissolve.c`. It prints how far the points have drifted and stops after 600
frames. If no window can be opened, `frame` returns 0 and the program ends.
