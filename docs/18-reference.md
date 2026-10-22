# 18. Language Reference

A compact summary of the whole language. For explanations and examples, see the guide
chapters linked from the [docs index](README.md).

## Source files

- Extension `.kl`, encoded as UTF-8.
- A file contains `import`, `fn`, `class`, `interface` and `enum` declarations, in any order.
- Exactly one `fn main()` (across all imported files) with no parameters and no return type.

## Lexical structure

| Element | Form |
|---|---|
| Line comment | `// ...` |
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
fn  class  interface  enum  import  init  override  private  static  abstract
let  var  if  else  while  for  in  match  return  break  continue
true  false  none  self  super  as  is
```

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
| `Name<T, ...>` | instance of a generic class | none — must be given |
| `T` | type parameter inside generic code | — (stored as empty until set) |
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
| 7 | `-x` `!x` | numeric / `bool` |
| 8 | `x as C` `x is C` | object → `C?` / `bool` |
| 9 | `f(...)` `x.m(...)` `x.f` `x[i]` | postfix |

Assignment operators (statements only): `=` `+=` `-=` `*=` `/=` `%=` (`+=` works on `str`).

## Statements

```text
let NAME [: TYPE] = EXPR
var NAME [: TYPE] = EXPR
TARGET = EXPR                  TARGET is a var, self.field, obj.field or a[i]
TARGET op= EXPR
if EXPR { ... } [else if EXPR { ... }] [else { ... }]
if let NAME = OPTIONAL_EXPR { ... } [else { ... }]
match EXPR { PATTERN => STATEMENT_OR_BLOCK ... }
    PATTERN = Variant | Variant(name, _, ...) | Enum.Variant(...) | INT | STRING | true | false | _
while EXPR { ... }
for NAME in EXPR..EXPR { ... }
for NAME in ARRAY_OR_STR { ... }
return [EXPR]
break
continue
EXPR                           usually a call
```

## Declarations

```text
fn NAME(PARAM, ...) [-> TYPE] { ... }
    PARAM = NAME: TYPE

import "PATH"

[abstract] class NAME[<T, ...>] [: BASE_OR_INTERFACE, ...] {
    [private] FIELD: TYPE [= EXPR]
    init(PARAM, ...) { ... }                         at most one
    [private] [static] fn NAME(PARAM, ...) [-> TYPE] { ... }
    [private] override fn NAME(PARAM, ...) [-> TYPE] { ... }
    abstract fn NAME(PARAM, ...) [-> TYPE]           abstract classes only
}

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
| `print(values...)` | space-separated, newline at the end; objects are not printable |
| `len(x: [T] or str) -> int` | characters for `str` |
| `str(x) -> str` | `int`, `float`, `bool`, `str`, arrays and optionals of these |
| `int(x) -> int` | from `float` (truncates), `bool`, `str` (runtime error if invalid) |
| `float(x) -> float` | from `int`, `str` |
| `input([prompt: str]) -> str` | one line from stdin, without the line ending |

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

## Value display

| Value | Printed as |
|---|---|
| `42` | `42` |
| `2.0`, `0.25` | `2.0`, `0.25` |
| `true` | `true` |
| `"hi"` | `hi` |
| `[1, 2]`, `["a"]` | `[1, 2]`, `[a]` |
| an `int?` holding `none` / `5` | `none` / `5` |

## Grammar (EBNF)

```text
program     = { import | function | class | interface | enum } ;
import      = "import" STRING ;
function    = "fn" IDENT [ tparams ] params [ "->" type ] block ;
tparams     = "<" IDENT { "," IDENT } ">" ;
params      = "(" [ param { "," param } [ "," ] ] ")" ;
param       = IDENT ":" type ;
class       = [ "abstract" ] "class" IDENT [ tparams ] [ ":" IDENT { "," IDENT } ] "{" { member } "}" ;
member      = { modifier } IDENT ":" type [ "=" expr ]
            | "init" params block
            | { modifier } "fn" IDENT params [ "->" type ] ( block | (* abstract *) ) ;
modifier    = "private" | "static" | "abstract" | "override" ;
interface   = "interface" IDENT [ ":" IDENT { "," IDENT } ] "{" { "fn" IDENT params [ "->" type ] } "}" ;
enum        = "enum" IDENT "{" variant { [ "," ] variant } "}" ;
variant     = IDENT [ params ] ;
type        = base [ "?" ] ;
base        = "int" | "float" | "bool" | "str" | "void" | IDENT [ "<" type { "," type } ">" ]
            | "[" type "]" | "{" type ":" type "}" | "fn" "(" [ type { "," type } ] ")" [ "->" type ] ;

block       = "{" { statement } "}" ;
statement   = ( "let" | "var" ) IDENT [ ":" type ] "=" expr
            | expr [ assign_op expr ]
            | "if" ( expr | "let" IDENT "=" expr ) block [ "else" ( block | if_stmt ) ]
            | "while" expr block
            | "for" IDENT "in" expr [ ".." expr ] block
            | "match" expr "{" { pattern "=>" ( block | statement ) } "}"
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
unary       = ( "-" | "!" ) unary | cast ;
cast        = postfix { ( "as" | "is" ) IDENT } ;
postfix     = primary { "." IDENT [ args ] | "[" expr "]" } ;
primary     = INT | FLOAT | STRING | "true" | "false" | "none" | "self"
            | IDENT [ "<" type { "," type } ">" ] [ args ]
            | "super" ( args | "." IDENT args )
            | "fn" "(" [ IDENT [ ":" type ] { "," IDENT [ ":" type ] } ] ")" [ "->" type ] ( "=>" expr | block )
            | "(" expr ")" | "[" [ expr { "," expr } [ "," ] ] "]"
            | "{" [ expr ":" expr { "," expr ":" expr } [ "," ] ] "}" ;
args        = "(" [ expr { "," expr } [ "," ] ] ")" ;
```

## Not yet supported

Bounds on type parameters (`T: Show`), generic interfaces and enums, inheritance with
generic classes, closures that change captured variables, global variables, module
namespaces (`import ... as x`), access control for top-level declarations, operator
overloading, string indexing, tuples, exceptions (`try`/`catch`), concurrency.

---

Previous: [17. How It Works](17-how-it-works.md) · Back to the [docs index](README.md)
