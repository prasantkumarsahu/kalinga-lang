# 26. Language Reference

A compact summary of the whole language. For explanations and examples, see the guide
chapters linked from the [docs index](README.md).

## Source files

- Extension `.kl`, encoded as UTF-8.
- A file contains `import`, `use`, `extern`, `fn`, `class`, `struct`, `interface` and `enum`
  declarations, in any order.
- Exactly one `fn main()` (across all imported files) with no parameters and no return type.

## Lexical structure

| Element | Form |
|---|---|
| Line comment | `// ...` |
| Doc comment | `/// ...` directly above a declaration; Markdown; shown by editors |
| Block comment | `/* ... */` (not nested) |
| Identifier | letter or `_`, then letters, digits, `_`; must not start with `__` |
| Integer | `123`, `1_000_000` (64-bit; `_` ignored) |
| Float | `1.5`, `0.25` (digits on both sides of `.`) |
| String | `"..."` with escapes `\n \t \r \0 \\ \"`; `{expr}` interpolation, `{{` `}}` literal braces; single line |
| Statement end | newline or `;` |

A newline does **not** end a statement when it follows an operator, `,`, `.`, `:`, `->`,
`{`, or occurs inside `( )` or `[ ]`.

## Keywords

```text
fn  class  struct  interface  enum  const  import  use  extern  init  override  private  static  abstract
let  var  if  else  while  for  in  match  return  break  continue  async  await
true  false  none  self  super  as  is
```

Contextual words (only special in one place): `mut` before `fn` and `op` before an
operator inside a struct; `rust` and `crate` after `use` and `rust` after `extern fn ... =`;
`type` after `extern`; `features` after a crate version.

Built-in type names: `int`, `float`, `bool`, `str`, `void`.

Reserved names (cannot be used as class or interface names): the built-in types and
functions, plus `Rc`, `RefCell`, `String`, `Vec`, `Option`, `Some`, `None`, `Box`,
`Self`, `std`, `main`.

## Types

| Syntax | Meaning | Default value |
|---|---|---|
| `int` | 64-bit signed integer | `0` |
| `float` | 64-bit IEEE float | `0.0` |
| `bool` | `true` / `false` | `false` |
| `str` | UTF-8 text | `""` |
| `[T]` | array (value semantics) | `[]` |
| `T?` | optional; `T` or `none` (`T??` not allowed) | `none` |
| `{K: V}` | map; keys `int`, `str` or `bool`; sorted by key | `{}` |
| `fn(A, B) -> R` | function value | none — must be given |
| `Name<T, ...>` | instance of a generic class or struct | none — must be given |
| `Result<T, E>` | `Ok(T)` or `Err(E)` (chapter 16) | none — must be given |
| `Task<T>` | a running thread (chapter 20) | none — must be given |
| `Channel<T>`, `Shared<T>` | values shared between threads (chapter 20) | none — must be given |
| `Async<T>` | a running async job (chapter 21) | none — must be given |
| `T` | type parameter inside generic code | — (stored as empty until set) |
| struct `Name` | value type, copied on assignment | none — must be given |
| `extern type Name` | opaque Rust type | none — must be given |
| `Name` | class or interface (reference semantics) | none — must be given |
| `void` | no value; return types only | — |

### Assignability

A value of type `A` can be used where `B` is expected when:

- `A` and `B` are the same type
- `B` is `T?` and `A` is assignable to `T`, or `A` is `none`
- `A` is a class that is `B` or a subclass of `B`
- `B` is an interface implemented by class `A` or one of its base classes
- `B` is an interface that `A`'s interface extends
- both are arrays (or optionals) whose element types are assignable, or maps with the same
  key type and assignable values

No other conversions happen automatically.

## Operator precedence

From lowest to highest. All binary operators are left-associative.

| Level | Operators | Operand types |
|---|---|---|
| 1 | `\|\|` | `bool` |
| 2 | `&&` | `bool` |
| 3 | `==` `!=` | same type: `int`, `float`, `bool`, `str`, arrays/optionals of these; `T? == none` |
| 4 | `<` `<=` `>` `>=` | same type: `int`, `float`, `str` |
| 5 | `+` `-` | `int`, `float`; `+` also joins `str` |
| 6 | `*` `/` `%` | `int`, `float` |
| 7 | `-x` `!x` `await x` | numeric / `bool` / `Async<T>` or `Task<T>` (inside `async fn` only) |
| 8 | `x as C` `x is C` | object → `C?` / `bool` |
| 9 | `f(...)` `x.m(...)` `x.f` `x[i]` `x?` | postfix; `?` unwraps a `Result` / optional or returns early |

Assignment operators (statements only): `=` `+=` `-=` `*=` `/=` `%=` (`+=` works on `str`).

## Statements

```text
let NAME [: TYPE] = EXPR
var NAME [: TYPE] = EXPR
TARGET = EXPR                  TARGET is a var, self.field, obj.field or a[i]
TARGET op= EXPR
if EXPR { ... } [else if EXPR { ... }] [else { ... }]
if let NAME = OPTIONAL_EXPR { ... } [else { ... }]
match EXPR { PATTERN => STATEMENT_OR_BLOCK ... }     arms on one line need `,` between them
    PATTERN = Variant | Variant(name, _, ...) | Enum.Variant(...) | INT | STRING | true | false | _
while EXPR { ... }
while let NAME = OPTIONAL_EXPR { ... }
for NAME in EXPR..EXPR { ... }                       EXPR..=EXPR includes the end
for NAME in ARRAY_OR_STR_OR_MAP { ... }              a map gives its keys
for INDEX, NAME in ARRAY_OR_STR { ... }              position from 0, then element
for KEY, VALUE in MAP { ... }
return [EXPR]
break
continue
EXPR                           usually a call
```

## Declarations

```text
fn NAME(PARAM, ...) [-> TYPE] { ... }
    PARAM = NAME: TYPE
async fn NAME(PARAM, ...) [-> TYPE] { ... }          calling it gives Async<TYPE>
const NAME: TYPE = EXPR                              a constant

import "FILE.kl"                                     a file, relative to this one
import "std/MODULE"                                  a standard module
import "PACKAGE[/FILE]" [as ALIAS]                   a utkal package; ALIAS.Name

[abstract] class NAME[<T, ...>] [: BASE_OR_INTERFACE, ...] {
    [private] FIELD: TYPE [= EXPR]
    const NAME: TYPE = EXPR                          a constant, used as Class.NAME
    static var NAME: TYPE = EXPR                     one variable for the class, Class.NAME
    init(PARAM, ...) { ... }                         at most one
    [private] [static] fn NAME(PARAM, ...) [-> TYPE] { ... }
    [private] static async fn NAME(PARAM, ...) [-> TYPE] { ... }
    [private] override fn NAME(PARAM, ...) [-> TYPE] { ... }   from a base class or interface
    abstract fn NAME(PARAM, ...) [-> TYPE]           abstract classes only
}

struct NAME[<T, ...>] {
    [private] FIELD: TYPE [= EXPR]
    const NAME: TYPE = EXPR                          a constant, used as Struct.NAME
    static var NAME: TYPE = EXPR                     one variable for the struct, Struct.NAME
    [private] [static] fn NAME(PARAM, ...) [-> TYPE] { ... }
    [private] mut fn NAME(PARAM, ...) [-> TYPE] { ... }     may change `self`
    op OPERATOR(PARAM) -> TYPE { ... }               + - * / %; `op -()` is unary minus
}

use rust "FILE.rs"
use crate "NAME" = "VERSION" [features ["FEATURE", ...]]
extern type NAME
extern fn NAME(PARAM, ...) [-> TYPE] [= "rust::path" | = rust { RUST CODE }]

interface NAME [: INTERFACE, ...] {
    fn NAME(PARAM, ...) [-> TYPE]
}

enum NAME {
    VARIANT [(PARAM, ...)]                           one per line or comma-separated
}

fn NAME<T, ...>(PARAM, ...) [-> TYPE] { ... }       generic function
```

Inside `init` and methods: `self.field`, `self.method(...)`, `super(...)` (first statement
of `init`, required when the base has an `init`), `super.method(...)`.

## Built-in functions

| Signature | Notes |
|---|---|
| `print(values...)` | space-separated, no newline (the text appears at once); objects are not printable |
| `println(values...)` | like `print`, then a newline; `println()` prints an empty line |
| `len(x: [T] or str) -> int` | characters for `str` |
| `str(x) -> str` | `int`, `float`, `bool`, `str`, arrays and optionals of these |
| `int(x) -> int` | from `float` (truncates), `bool`, `str` (runtime error if invalid) |
| `float(x) -> float` | from `int`, `str` |
| `input([prompt: str]) -> str` | one line from stdin, without the line ending |
| `chr(code: int) -> str` | the character with that code point: `chr(65)` is `"A"` |
| `args() -> [str]` | the command-line arguments, without the program's own name |

`Math`, `Random` and `Time` functions are listed in [Standard Library](15-standard-library.md).

## Built-in methods

| Receiver | Method | Result |
|---|---|---|
| `[T]` (var) | `push(x: T)` | — |
| `[T]` (var) | `pop()` | `T?` |
| `[T]` | `len()` | `int` |
| `[T]` | `contains(x: T)` | `bool` (simple element types only) |
| `[T]` | `map(f: fn(T) -> U)` | `[U]` |
| `[T]` | `filter(f: fn(T) -> bool)` | `[T]` |
| `[T]` | `find(f: fn(T) -> bool)` | `T?` |
| `[T]` (var) | `sort()` | — (`int`, `float`, `str`, `bool`) |
| `[T]` (var) | `sort_by(f: fn(T, T) -> bool)` | — |
| `{K: V}` | `get(k)` / `has(k)` | `V?` / `bool` |
| `{K: V}` (var) | `remove(k)` | `V?` |
| `{K: V}` | `keys()` / `values()` / `len()` | `[K]` / `[V]` / `int` |
| `str` | `len()` | `int` |
| `str` | `upper()` / `lower()` / `trim()` | `str` |
| `str` | `contains(s: str)` | `bool` |
| `str` | `split(sep: str)` | `[str]` (characters when `sep` is `""`) |
| `str` | `replace(from: str, to: str)` | `str` |
| `str` | `starts_with(s: str)` / `ends_with(s: str)` | `bool` |
| `str` | `find(s: str)` | `int?`, the index of the first `s` |
| `str` | `code_at(i: int)` | `int`, the code point at index `i` |
| `str` | `chars()` | `[str]` |
| `str` | `parse_int()` / `parse_float()` | `int?` / `float?` |
| `[T]` | `join(sep: str)` | `str` (arrays of `str`) |
| `[T]` | `par_map(f: fn(T) -> U)` | `[U]`, computed on all cores (values only) |
| `Result<T, E>` | `is_ok()` / `is_err()` | `bool` |
| `Result<T, E>` | `ok()` / `err()` | `T?` / `E?` |
| `Result<T, E>` | `unwrap_or(x: T)` / `unwrap()` | `T` (`unwrap` stops the program on `Err`) |
| `Task<T>` | `join()` / `is_done()` | `T` / `bool` |
| `Channel<T>` | `send(x: T)` / `close()` | — |
| `Channel<T>` | `receive()` / `try_receive()` | `T?` (`receive` waits) |
| `Shared<T>` | `get()` / `set(x: T)` / `update(f: fn(T) -> T)` | `T` / — / — |

Library functions: `Math.*`, `Random.*` and `Time.*` (chapter 15), `Time.wait(s)`,
`Async.spawn/all/run` (chapter 21), `Thread.spawn(f)` and `Thread.cores()` (chapter 20).

## Value display

| Value | Printed as |
|---|---|
| `42` | `42` |
| `2.0`, `0.25` | `2.0`, `0.25` |
| `true` | `true` |
| `"hi"` | `hi` |
| `[1, 2]`, `["a"]` | `[1, 2]`, `[a]` |
| an `int?` holding `none` / `5` | `none` / `5` |
| `Ok(3)`, `Err("no")` | `Ok(3)`, `Err(no)` |

## Grammar (EBNF)

```text
program     = { import | use | extern | function | class | struct | interface | enum | const } ;
const       = "const" IDENT ":" type "=" expr ;
import      = "import" STRING [ "as" IDENT ] ;
use         = "use" ( "rust" STRING | "crate" STRING "=" STRING [ "features" "[" STRING { "," STRING } "]" ] ) ;
extern      = "extern" ( "type" IDENT | "fn" IDENT params [ "->" type ] [ "=" ( STRING | "rust" RUST_BLOCK ) ] ) ;
struct      = "struct" IDENT [ tparams ] "{" { { modifier } IDENT ":" type [ "=" expr ]
            | const | "static" "var" IDENT ":" type "=" expr
            | [ "mut" ] { modifier } "fn" IDENT params [ "->" type ] block
            | "op" ( "+" | "-" | "*" | "/" | "%" ) params "->" type block } "}" ;
function    = [ "async" ] "fn" IDENT [ tparams ] params [ "->" type ] block ;
tparams     = "<" IDENT { "," IDENT } ">" ;
params      = "(" [ param { "," param } [ "," ] ] ")" ;
param       = IDENT ":" type ;
class       = [ "abstract" ] "class" IDENT [ tparams ] [ ":" IDENT { "," IDENT } ] "{" { member } "}" ;
member      = { modifier } IDENT ":" type [ "=" expr ]
            | "init" params block
            | const | "static" "var" IDENT ":" type "=" expr
            | { modifier } "fn" IDENT params [ "->" type ] ( block | (* abstract *) ) ;
modifier    = "private" | "static" | "abstract" | "override" | "async" ;
interface   = "interface" IDENT [ ":" IDENT { "," IDENT } ] "{" { "fn" IDENT params [ "->" type ] } "}" ;
enum        = "enum" IDENT "{" variant { [ "," ] variant } "}" ;
variant     = IDENT [ params ] ;
type        = base [ "?" ] ;
base        = "int" | "float" | "bool" | "str" | "void" | [ IDENT "." ] IDENT [ "<" type { "," type } ">" ]
            | "[" type "]" | "{" type ":" type "}" | "fn" "(" [ type { "," type } ] ")" [ "->" type ] ;

block       = "{" { statement } "}" ;
statement   = ( "let" | "var" ) IDENT [ ":" type ] "=" expr
            | expr [ assign_op expr ]
            | "if" ( expr | "let" IDENT "=" expr ) block [ "else" ( block | if_stmt ) ]
            | "while" [ "let" IDENT "=" ] expr block
            | "for" IDENT [ "," IDENT ] "in" expr [ ( ".." | "..=" ) expr ] block
            | "match" expr "{" { pattern "=>" ( block | statement ) [ "," ] } "}"
            | "return" [ expr ] | "break" | "continue" ;
pattern     = "_" | [ IDENT "." ] IDENT [ "(" IDENT { "," IDENT } ")" ] | [ "-" ] INT | STRING | "true" | "false" ;
assign_op   = "=" | "+=" | "-=" | "*=" | "/=" | "%=" ;

expr        = or ;
or          = and { "||" and } ;
and         = equality { "&&" equality } ;
equality    = comparison { ( "==" | "!=" ) comparison } ;
comparison  = additive { ( "<" | "<=" | ">" | ">=" ) additive } ;
additive    = term { ( "+" | "-" ) term } ;
term        = unary { ( "*" | "/" | "%" ) unary } ;
unary       = ( "-" | "!" | "await" ) unary | cast ;
cast        = postfix { ( "as" | "is" ) IDENT } ;
postfix     = primary { "." IDENT [ args ] | "[" expr "]" | "[" [ expr ] ( ".." | "..=" ) [ expr ] "]" | "?" } ;
primary     = INT | FLOAT | STRING | "true" | "false" | "none" | "self"
            | IDENT [ "<" type { "," type } ">" ] [ args ]
            | "super" ( args | "." IDENT args )
            | "fn" "(" [ IDENT [ ":" type ] { "," IDENT [ ":" type ] } ] ")" [ "->" type ] ( "=>" expr | block )
            | "if" expr value_block "else" ( value_block | (* if expression *) )
            | "match" expr "{" { pattern "=>" ( value_block | expr ) [ "," ] } "}"
            | "(" expr ")" | "[" [ expr { "," expr } [ "," ] ] "]"
            | "{" [ expr ":" expr { "," expr ":" expr } [ "," ] ] "}" ;
args        = "(" [ expr { "," expr } [ "," ] ] ")" ;
value_block = "{" { statement } expr "}" ;
```

## Not yet supported

Bounds on type parameters (`T: Show`), generic interfaces and enums, inheritance with
generic classes, operators on classes, closures that change captured variables, mutable
global variables (use a `static var` member), `if let` as a value, `async` instance
methods, `await` inside lambdas, sharing objects between threads, a package registry,
tuples, exceptions (`try`/`catch`; use `Result`).

---

Previous: [25. How It Works](25-how-it-works.md)
