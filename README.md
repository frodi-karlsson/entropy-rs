# entropy-rs

A compiler for [Entropy](https://entropy-lang.org/) ([esolangs](https://esolangs.org/wiki/Entropy)),
Daniel Temkin's language in which data decays every time it is used.

```entropy
Program hello world [
    print "Hello, World!\n";
]
```

Most runs print it intact. Some don't: seed 0 prints `Hellm, World!` and seed 3 prints
`Hello,!World!`.

## Install

```sh
curl -LsSf https://github.com/frodi-karlsson/entropy-rs/releases/latest/download/entropy-lang-installer.sh | sh
brew install frodi-karlsson/tap/entropy-lang
cargo install entropy-lang --locked
```

Prebuilt binaries cover macOS and Linux on arm64 and x86_64. `check` and `run` need nothing else.
`build` needs a system C linker (Xcode Command Line Tools, or `build-essential`).

## Use

```sh
entropy check hello.en                  # report errors, run nothing
entropy run hello.en                    # compile in memory and run
entropy build hello.en -o hello         # write a native executable
entropy dump ast|hir hello.en           # print the syntax tree or typed program
```

`run` takes `--seed N`, `--mutation-rate R` and `--fuel N`. `build` bakes the rate into the
executable, which reads `ENTROPY_SEED` and `ENTROPY_FUEL` instead.

- **Rate** is how far one decay step can move a value. The default is 2. `0` turns decay off.
  Anything from 0.001 to 100 is allowed.
- **Seed** fixes the randomness. The same program, rate, seed and fuel give byte-identical output
  from `run` and from a built executable. Without a seed, the OS picks one.
- **Fuel** caps the number of decay steps. Running out prints `entropy: out of fuel` and exits with
  code 4. There is no cap by default.

Exit codes: 0 success, 1 compile errors, 2 bad usage, 3 link or load failure, 4 out of fuel, 101
internal error.

## The language

A program is a header and a block. There are three types: `real` (an f32 value), and `char` and
`string`, which are shared objects.

```entropy
Program examples countdown [
    # Declarations come in two forms.
    declare count real;
    let count = 3;
    string label = " left\n";
    while count > 0 [
        print count + label;
        let count -= 1;
    ]
]
```

At seed 1 this prints four lines, not three. The count never lands on exactly 0: its last value is
`0.031002156`.

- Statements: `declare`, `type name = value;`, `let` (with `=`, `+=`, `-=`, `*=`, `/=` or `%=`),
  `print`, `if`, `while`, and calls to externs. Blocks use `[ ]`.
- Operators: `+ - * / %` on numbers, `+` to join strings (with strings, chars or reals), and
  `< > <= >=` in conditions. Comparisons can't be chained or stored. There is no `==`, no logical
  operators and no unary minus: write `0 - 1`.
- Comments: `//`, `#` and `/* */`. Names are case-insensitive. Keywords are reserved.

### Decay

Every read of a value takes one decay step, and the step stays. A step adds or subtracts
`rate / k` for a random `k` from 1 to 255. This applies to variables, to the right-hand side of an
assignment, to temporaries like `a + b`, and to literals: each literal in the source has its own
hidden slot, so the `0` in `while count > 0` drifts a little on every pass. A string decays one step
per character, and a char displays as the nearest code point.

Values are read left to right, each one fully before the next. That order fixes the random draws,
which is why a seed reproduces a run exactly.

### Externs

`extern` calls C functions. Values decay while they're in Entropy and arrive in C as they were when
passed: a `real` as a `double`, a `char` as an `int32_t` code point, a `string` as UTF-8.

```entropy
Program examples roots [
    extern sqrt(real) returns real;
    print sqrt(2);
]
```

`run` finds symbols in each `--dylib PATH`, then in the process and libm. `build` takes `-l` and
`-L` like a C compiler. Externs must not be variadic. `examples/mandelbrot` draws a PPM through a C
`plot` function.

## Where this implementation defines behaviour

The written spec leaves some things open. These are the rules here.

- Parentheses group as written, and precedence is `* / %` over `+ -` over comparisons.
- Compound assignment really computes: `let x += e` is `let x = x + e`. Bitwise compound operators
  are rejected.
- Every read decays the stored value, including reals and real literals.
- All arithmetic works between any numeric types: `* / %`, `<=` and `>=` included.
- Reading a variable before it is assigned is a compile error.
- Names are case-insensitive everywhere.
- A char that isn't a valid code point (NaN, infinity, a surrogate, out of range) displays as
  U+FFFD.
- Reals print in their shortest round-trip form: `0.1`, `inf`, `NaN`, `-0`.
- A string's cells are Unicode scalar values, so an emoji is one cell.
- The left operand is consumed before the right one is evaluated.
- `#` comments and `type name = value;` declarations work, as on entropy-lang.org.
- Randomness is seedable, and fuel can cap a run.

## Licence

MIT
