# entropy-rs

A compiler for [Entropy](https://entropy-lang.org/), Daniel Temkin's programming language where
data falls apart a little every time you use it. It builds native executables, runs programs
straight from source, and lets them call C libraries.

![The word ENTROPY as glowing points, blurring and scattering as it decays](assets/dissolve.gif)

This is [examples/dissolve](examples/dissolve). The whole word is one string, and every time the
program reads it, every point drifts a little.

Here is the same Mandelbrot program run twice. The first run has decay turned off.

```text
                                          #####
                                           ####
                                   ################
                                    ##################
                                  #####################
                           #     ######################
                       #######  #######################
                     ###################################
     ################################################
                     ###################################
                       #######  #######################
                           #     ######################
                                  #####################
                                    ##################
                                   ################
                                           ####
                                          #####
```

The second run lets it decay. Every number in the program drifted a little each time it was read,
and the picture melted.

```text
                                             ###
                                     ################# ##
                                    ###################
                                    #####################
                                    #####################
                        ########   ####################
                      ########### ###################
                        ####       ###############
                         ##        ################
                                 ###################
                             # ##################
                                 ################
                                  ###########
                                    ###  #
                                      # #
```

Try it yourself:

```sh
entropy run examples/mandelbrot/ascii.en --mutation-rate 0.05 --seed 1
```

## Install

```sh
brew install frodi-karlsson/tap/entropy-lang
```

Or:

```sh
curl -LsSf https://github.com/frodi-karlsson/entropy-rs/releases/latest/download/entropy-lang-installer.sh | sh
cargo install entropy-lang --locked
```

Prebuilt binaries cover macOS and Linux on arm64 and x86_64. Building native executables also
needs a C linker (Xcode Command Line Tools, or `build-essential`).

## Hello, World

```entropy
Program hello world [
    print "Hello, World!\n";
]
```

```sh
entropy run hello.en             # run it now
entropy build hello.en -o hello  # or make a native executable
```

Most runs print `Hello, World!`. Some print `Hellm, World!` or `Hfllo, World!`. Read the same
string over and over and it keeps getting worse:

```entropy
Program hello again [
    string greeting = "Hello, World!\n";
    real i = 0;
    while i < 8 [
        print greeting;
        let i += 1;
    ]
]
```

```text
$ entropy run again.en --seed 25
Hello, World!
Hello, World!
Hello, Woqld!
Hello, Woqld!
Hello, Vorld!
Helko, Voqlc!
Helko, Voqlc!
Helko, Voqlc!
```

## Count to zero, if you can

```entropy
Program examples countdown [
    declare count real;
    let count = 3;
    string label = " left\n";
    while count > 0 [
        print count + label;
        let count -= 1;
    ]
]
```

```text
$ entropy run countdown.en --seed 1
3.0897207 left
2.0250254 left
1.0693203 left
0.031002156 left
```

It counted down from 3 and printed four lines. Each subtraction was a little off, so after three
steps the count was 0.03, not 0, and the loop went around once more.

## How decay works

Every time a value is read, it moves a small random amount, and the change sticks. Numbers drift.
Letters slide to nearby letters. Even the literal `0` in `while count > 0` drifts a little on every
pass.

Three options control it:

| Option | What it does |
| --- | --- |
| `--mutation-rate R` | How far one step can move a value. Default 2. `0` turns decay off. |
| `--seed N` | Replays the exact same run. Without it, every run is different. |
| `--fuel N` | Stops after N decay steps, with exit code 4. |

A built executable keeps its rate and reads `ENTROPY_SEED` and `ENTROPY_FUEL` from the environment.
The same seed gives the same output from `run` and from a built executable.

## Calling C

Programs can call C functions. Entropy's decay doesn't reach inside libraries.

```entropy
Program examples roots [
    extern sqrt(real) returns real;
    print sqrt(2);
]
```

`run` finds functions in each `--dylib PATH`, then in the process and libm. `build` takes `-l` and
`-L` like a C compiler. [examples/mandelbrot](examples/mandelbrot) uses this to draw a full colour
image. [examples/dissolve](examples/dissolve) draws the animation at the top
of this page.

## The language in brief

There are three types: `real` (a number), `char` and `string`. A program is a header and a block.

- Declare with `declare x real;` or `real x = 1;`. Assign with `let`, which also takes `+=`, `-=`,
  `*=`, `/=` and `%=`.
- Control flow is `if` and `while`. Blocks use `[ ]`.
- `+` adds numbers and joins strings. Comparisons are `< > <= >=`. There is no `==`, since values
  are never exactly equal anyway.
- Comments are `//`, `#` and `/* */`. Names ignore case.

The full language is described at [entropy-lang.org](https://entropy-lang.org/) and on
[esolangs.org](https://esolangs.org/wiki/Entropy). `entropy check FILE` reports errors without
running anything.

## Details

The descriptions on entropy-lang.org and esolangs.org leave some things open. These are the rules
here.

- Precedence is `* / %` over `+ -` over comparisons. Parentheses group as written.
- `let x += e` means `let x = x + e`. Bitwise compound operators are rejected.
- Every read decays the stored value, including real literals. A string decays one step per
  character, and a char shows as the nearest character.
- Values are read left to right, each fully before the next. This is what makes a seed reproducible.
- All arithmetic works between any numeric types.
- Reading a variable before assigning it is a compile error.
- A char that isn't a valid character (NaN, infinity, out of range) shows as U+FFFD.
- Reals print in their shortest exact form: `0.1`, `inf`, `NaN`, `-0`.
- An emoji is one character in a string.
- Externs must not be variadic. A `real` arrives as a `double`, a `char` as an `int32_t` code
  point, a `string` as UTF-8.

Exit codes: 0 success, 1 compile errors, 2 bad usage, 3 link or load failure, 4 out of fuel, 101
internal error.

## Licence

MIT
