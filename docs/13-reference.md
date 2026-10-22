# 13. Language Reference

A compact summary of the whole language. For explanations and examples, see the guide
chapters linked from the [docs index](README.md).

## Source files

- Extension `.kl`, encoded as UTF-8.
- A file contains `fn`, `class` and `interface` declarations, in any order.
- Exactly one `fn main()` with no parameters and no return type.

## Lexical structure

| Element | Form |
|---|---|
| Line comment | `// ...` |
| Block comment | `/* ... */` (not nested) |
| Identifier | letter or `_`, then letters, digits, `_`; must not start with `__` |
| Integer | `123`, `1_000_000` (64-bit; `_` ignored) |
| Float | `1.5`, `0.25` (digits on both sides of `.`) |
| String | `"..."` with escapes `\n \t \r \0 \\ \"`; single line |
| Statement end | newline or `;` |

A newline does **not** end a statement when it follows an operator, `,`, `.`, `:`, `->`,
`{`, or occurs inside `( )` or `[ ]`.

## Keywords

```text
fn  class  interface  init  override  let  var
if  else  while  for  in  return  break  continue
true  false  none  self  super
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
| `Name` | class or interface (reference semantics) | none — must be given |
| `void` | no value; return types only | — |

### Assignability

A value of type `A` can be used where `B` is expected when:

- `A` and `B` are the same type
- `B` is `T?` and `A` is assignable to `T`, or `A` is `none`
- `A` is a class that is `B` or a subclass of `B`
- `B` is an interface implemented by class `A` or one of its base classes
- both are arrays (or optionals) whose element types are assignable

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
| 8 | `f(...)` `x.m(...)` `x.f` `x[i]` | postfix |

Assignment operators (statements only): `=` `+=` `-=` `*=` `/=` `%=` (`+=` works on `str`).

## Statements

```text
let NAME [: TYPE] = EXPR
var NAME [: TYPE] = EXPR
TARGET = EXPR                  TARGET is a var, self.field, obj.field or a[i]
TARGET op= EXPR
if EXPR { ... } [else if EXPR { ... }] [else { ... }]
if let NAME = OPTIONAL_EXPR { ... } [else { ... }]
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

class NAME [: BASE_OR_INTERFACE, ...] {
    FIELD: TYPE [= EXPR]
    init(PARAM, ...) { ... }               at most one
    fn NAME(PARAM, ...) [-> TYPE] { ... }
    override fn NAME(PARAM, ...) [-> TYPE] { ... }
}

interface NAME {
    fn NAME(PARAM, ...) [-> TYPE]
}
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
program     = { function | class | interface } ;
function    = "fn" IDENT params [ "->" type ] block ;
params      = "(" [ param { "," param } [ "," ] ] ")" ;
param       = IDENT ":" type ;
class       = "class" IDENT [ ":" IDENT { "," IDENT } ] "{" { member } "}" ;
member      = IDENT ":" type [ "=" expr ]
            | "init" params block
            | [ "override" ] function ;
interface   = "interface" IDENT "{" { "fn" IDENT params [ "->" type ] } "}" ;
type        = ( "int" | "float" | "bool" | "str" | "void" | IDENT | "[" type "]" ) [ "?" ] ;

block       = "{" { statement } "}" ;
statement   = ( "let" | "var" ) IDENT [ ":" type ] "=" expr
            | expr [ assign_op expr ]
            | "if" ( expr | "let" IDENT "=" expr ) block [ "else" ( block | if_stmt ) ]
            | "while" expr block
            | "for" IDENT "in" expr [ ".." expr ] block
            | "return" [ expr ] | "break" | "continue" ;
assign_op   = "=" | "+=" | "-=" | "*=" | "/=" | "%=" ;

expr        = or ;
or          = and { "||" and } ;
and         = equality { "&&" equality } ;
equality    = comparison { ( "==" | "!=" ) comparison } ;
comparison  = additive { ( "<" | "<=" | ">" | ">=" ) additive } ;
additive    = term { ( "+" | "-" ) term } ;
term        = unary { ( "*" | "/" | "%" ) unary } ;
unary       = ( "-" | "!" ) unary | postfix ;
postfix     = primary { "." IDENT [ args ] | "[" expr "]" } ;
primary     = INT | FLOAT | STRING | "true" | "false" | "none" | "self"
            | IDENT [ args ] | "super" ( args | "." IDENT args )
            | "(" expr ")" | "[" [ expr { "," expr } [ "," ] ] "]" ;
args        = "(" [ expr { "," expr } [ "," ] ] ")" ;
```

## Not yet supported

Generics, closures and first-class functions, global variables, modules/imports,
access modifiers (`private`), static methods, abstract methods, interface inheritance,
downcasting, string indexing, maps/dictionaries, string interpolation.

---

Previous: [12. How It Works](12-how-it-works.md) · Back to the [docs index](README.md)
