/**
 * Tree-sitter grammar for Kalinga (https://github.com/prasantkumarsahu/kalinga-lang),
 * used by editors such as Zed for highlighting, outlines, brackets and indentation.
 * The compiler has its own parser; this grammar follows docs/26-reference.md.
 *
 * Statements end at a newline in Kalinga. Here newlines are ordinary whitespace, which
 * is enough for an editor: the rare line that could join the next one is still
 * highlighted correctly.
 */

const PREC = {
  or: 1,
  and: 2,
  equality: 3,
  compare: 4,
  additive: 5,
  multiplicative: 6,
  unary: 7,
  cast: 8,
  postfix: 9,
};

const commaSep = (rule) => optional(commaSep1(rule));
const commaSep1 = (rule) => seq(rule, repeat(seq(",", rule)), optional(","));

module.exports = grammar({
  name: "kalinga",

  extras: ($) => [/\s/, $.line_comment, $.block_comment],

  word: ($) => $.identifier,

  conflicts: ($) => [
    [$.block, $.map_literal],
    // `a < b` or `first<int>(...)`: decided by what follows the `<`.
    [$._expression, $.generic_call],
  ],

  supertypes: ($) => [$._item, $._statement, $._expression, $._type],

  rules: {
    source_file: ($) => repeat($._item),

    _item: ($) =>
      choice(
        $.import_declaration,
        $.use_declaration,
        $.extern_type,
        $.extern_function,
        $.function_declaration,
        $.test_declaration,
        $.class_declaration,
        $.struct_declaration,
        $.interface_declaration,
        $.enum_declaration,
        $.const_declaration,
      ),

    // ---------------------------------------------------------------- comments

    line_comment: (_) => token(seq("//", /[^\n]*/)),
    block_comment: (_) => token(seq("/*", /[^*]*\*+([^/*][^*]*\*+)*/, "/")),

    // ---------------------------------------------------------- declarations

    import_declaration: ($) =>
      seq("import", field("path", $.string), optional(seq("as", field("alias", $.identifier)))),

    use_declaration: ($) =>
      seq(
        "use",
        choice(
          seq("rust", field("path", $.string)),
          seq(
            "crate",
            field("name", $.string),
            "=",
            field("version", $.string),
            optional(seq("features", "[", commaSep($.string), "]")),
          ),
        ),
      ),

    extern_type: ($) => seq("extern", "type", field("name", $.type_identifier)),

    extern_function: ($) =>
      seq(
        "extern",
        "fn",
        field("name", $.identifier),
        field("parameters", $.parameters),
        optional(seq("->", field("return_type", $._type))),
        optional(seq("=", choice(field("rust_path", $.string), $.rust_block))),
      ),

    rust_block: ($) => seq("rust", $.rust_code),
    // Rust code, with its braces balanced; braces inside Rust strings do not count.
    rust_code: ($) => seq("{", repeat(choice(/[^{}"]+/, /"([^"\\]|\\.)*"/, $.rust_code)), "}"),

    modifier: (_) => choice("private", "static", "abstract", "override", "async", "mut"),

    function_declaration: ($) =>
      seq(
        repeat($.modifier),
        "fn",
        field("name", $.identifier),
        optional(field("type_parameters", $.type_parameters)),
        field("parameters", $.parameters),
        optional(seq("->", field("return_type", $._type))),
        optional(field("body", $.block)),
      ),

    test_declaration: ($) => seq("test", "fn", field("name", $.identifier), "(", ")", field("body", $.block)),

    type_parameters: ($) => seq("<", commaSep1($.type_parameter), ">"),
    type_parameter: ($) =>
      seq(field("name", $.type_identifier), optional(seq(":", $._bound, repeat(seq("+", $._bound))))),
    _bound: ($) => choice($.generic_type, $.type_identifier),

    parameters: ($) => seq("(", commaSep($.parameter), ")"),
    parameter: ($) => seq(field("name", $.identifier), ":", field("type", $._type)),

    const_declaration: ($) =>
      seq("const", field("name", $.identifier), ":", field("type", $._type), "=", field("value", $._expression)),

    static_var: ($) =>
      seq(
        "static",
        "var",
        field("name", $.identifier),
        ":",
        field("type", $._type),
        "=",
        field("value", $._expression),
      ),

    class_declaration: ($) =>
      seq(
        optional("abstract"),
        "class",
        field("name", $.type_identifier),
        optional(field("type_parameters", $.type_parameters)),
        optional(seq(":", commaSep1($._bound))),
        field("body", $.class_body),
      ),

    class_body: ($) =>
      seq("{", repeat(choice($.field_declaration, $.init_declaration, $.function_declaration, $.const_declaration, $.static_var)), "}"),

    struct_declaration: ($) =>
      seq(
        "struct",
        field("name", $.type_identifier),
        optional(field("type_parameters", $.type_parameters)),
        field("body", $.struct_body),
      ),

    struct_body: ($) =>
      seq(
        "{",
        repeat(
          choice($.field_declaration, $.function_declaration, $.operator_declaration, $.const_declaration, $.static_var),
        ),
        "}",
      ),

    field_declaration: ($) =>
      seq(
        optional("private"),
        field("name", $.identifier),
        ":",
        field("type", $._type),
        optional(seq("=", field("default", $._expression))),
      ),

    init_declaration: ($) => seq("init", field("parameters", $.parameters), field("body", $.block)),

    operator_declaration: ($) =>
      seq(
        "op",
        field("operator", choice("+", "-", "*", "/", "%")),
        field("parameters", $.parameters),
        "->",
        field("return_type", $._type),
        field("body", $.block),
      ),

    interface_declaration: ($) =>
      seq(
        "interface",
        field("name", $.type_identifier),
        optional(field("type_parameters", $.type_parameters)),
        optional(seq(":", commaSep1($._bound))),
        field("body", $.interface_body),
      ),

    interface_body: ($) => seq("{", repeat($.method_signature), "}"),

    method_signature: ($) =>
      seq(
        "fn",
        field("name", $.identifier),
        field("parameters", $.parameters),
        optional(seq("->", field("return_type", $._type))),
      ),

    enum_declaration: ($) =>
      seq(
        "enum",
        field("name", $.type_identifier),
        optional(field("type_parameters", $.type_parameters)),
        field("body", $.enum_body),
      ),

    enum_body: ($) => seq("{", repeat(seq($.enum_variant, optional(","))), "}"),

    enum_variant: ($) => seq(field("name", $.type_identifier), optional(field("parameters", $.parameters))),

    // ----------------------------------------------------------------- types

    _type: ($) =>
      choice(
        $.primitive_type,
        $.type_identifier,
        $.scoped_type,
        $.generic_type,
        $.array_type,
        $.map_type,
        $.optional_type,
        $.function_type,
        $.tuple_type,
      ),

    primitive_type: (_) => choice("int", "float", "bool", "str", "void"),
    // One token for all names: whether `Name` is a type depends on where it is.
    type_identifier: ($) => $.identifier,
    scoped_type: ($) => seq(field("package", $.identifier), ".", field("name", $.type_identifier)),
    generic_type: ($) =>
      prec(1, seq(field("name", choice($.type_identifier, $.scoped_type)), $.type_arguments)),
    type_arguments: ($) => seq("<", commaSep1($._type), ">"),
    array_type: ($) => seq("[", $._type, "]"),
    map_type: ($) => seq("{", field("key", $._type), ":", field("value", $._type), "}"),
    optional_type: ($) => prec(1, seq($._type, "?")),
    function_type: ($) =>
      prec.right(seq("fn", "(", commaSep($._type), ")", optional(seq("->", field("return_type", $._type))))),
    tuple_type: ($) => seq("(", $._type, ",", commaSep1($._type), ")"),

    // ------------------------------------------------------------ statements

    block: ($) => seq("{", repeat(seq($._statement, optional(";"))), "}"),

    _statement: ($) =>
      choice(
        $.let_statement,
        $.assignment,
        $.return_statement,
        $.break_statement,
        $.continue_statement,
        $.while_statement,
        $.for_statement,
        $.expression_statement,
      ),

    let_statement: ($) =>
      seq(
        choice("let", "var"),
        field("pattern", choice($.identifier, $.tuple_pattern)),
        optional(seq(":", field("type", $._type))),
        "=",
        field("value", $._expression),
      ),

    tuple_pattern: ($) => seq("(", commaSep1($.identifier), ")"),

    assignment: ($) =>
      prec.right(
        seq(
          field("target", $._expression),
          field("operator", choice("=", "+=", "-=", "*=", "/=", "%=")),
          field("value", $._expression),
        ),
      ),

    return_statement: ($) => prec.right(seq("return", optional($._expression))),
    break_statement: (_) => "break",
    continue_statement: (_) => "continue",

    while_statement: ($) =>
      seq(
        "while",
        choice(field("condition", $._expression), seq("let", field("name", $.identifier), "=", field("value", $._expression))),
        field("body", $.block),
      ),

    for_statement: ($) =>
      seq(
        "for",
        choice(
          seq(optional(seq(field("index", $.identifier), ",")), field("variable", $.identifier)),
          field("variable", $.tuple_pattern),
        ),
        "in",
        field("iterable", $._expression),
        optional(seq(choice("..", "..="), field("end", $._expression))),
        field("body", $.block),
      ),

    expression_statement: ($) => prec(-1, $._expression),

    // ----------------------------------------------------------- expressions

    _expression: ($) =>
      choice(
        $.identifier,
        $.self,
        $.integer,
        $.float,
        $.boolean,
        $.none,
        $.string,
        $.array_literal,
        $.map_literal,
        $.tuple,
        $.parenthesized_expression,
        $.lambda,
        $.if_expression,
        $.match_expression,
        $.call_expression,
        $.generic_call,
        $.method_call,
        $.field_expression,
        $.index_expression,
        $.slice_expression,
        $.try_expression,
        $.cast_expression,
        $.unary_expression,
        $.binary_expression,
        $.super_call,
      ),

    self: (_) => "self",
    integer: (_) => token(/[0-9][0-9_]*/),
    float: (_) => token(/[0-9][0-9_]*\.[0-9][0-9_]*/),
    boolean: (_) => choice("true", "false"),
    none: (_) => "none",

    string: ($) =>
      seq(
        '"',
        repeat(choice($.string_content, $.escape_sequence, $.interpolation)),
        token.immediate('"'),
      ),
    string_content: (_) => token.immediate(prec(1, /[^"\\{\n]+/)),
    escape_sequence: (_) => token.immediate(choice(/\\[ntr0\\"]/, "{{", "}}")),
    interpolation: ($) => seq(token.immediate("{"), $._expression, "}"),

    array_literal: ($) => seq("[", commaSep($._expression), "]"),
    map_literal: ($) => seq("{", commaSep($.map_entry), "}"),
    map_entry: ($) => seq(field("key", $._expression), ":", field("value", $._expression)),
    tuple: ($) => seq("(", $._expression, ",", commaSep1($._expression), ")"),
    parenthesized_expression: ($) => seq("(", $._expression, ")"),

    lambda: ($) =>
      prec.right(
        seq(
          "fn",
          field("parameters", $.lambda_parameters),
          optional(seq("->", field("return_type", $._type))),
          choice(seq("=>", field("body", $._expression)), field("body", $.block)),
        ),
      ),
    lambda_parameters: ($) =>
      seq("(", commaSep(seq(field("name", $.identifier), optional(seq(":", field("type", $._type))))), ")"),

    if_expression: ($) =>
      prec.right(
        seq(
          "if",
          choice(field("condition", $._expression), seq("let", field("name", $.identifier), "=", field("value", $._expression))),
          field("consequence", $.block),
          optional(seq("else", field("alternative", choice($.block, $.if_expression)))),
        ),
      ),

    match_expression: ($) => seq("match", field("value", $._expression), "{", repeat($.match_arm), "}"),
    match_arm: ($) =>
      prec.right(
        seq(
          field("pattern", $.pattern),
          "=>",
          field("body", choice($.block, $._statement)),
          optional(","),
        ),
      ),
    pattern: ($) =>
      choice(
        "_",
        seq(
          optional(seq(field("enum", $.type_identifier), ".")),
          field("variant", $.type_identifier),
          optional(seq("(", commaSep(choice($.identifier, "_")), ")")),
        ),
        seq(optional("-"), $.integer),
        $.string,
        $.boolean,
      ),

    call_expression: ($) =>
      prec(PREC.postfix, seq(field("function", $.identifier), field("arguments", $.arguments))),
    // No precedence: `a < b` and `first<int>(...)` both stay possible until the code after
    // the `<` decides.
    generic_call: ($) => seq(field("function", $.identifier), $.type_arguments, field("arguments", $.arguments)),
    method_call: ($) =>
      prec(
        PREC.postfix + 1,
        seq(field("receiver", $._expression), ".", field("method", $.identifier), field("arguments", $.arguments)),
      ),
    field_expression: ($) =>
      prec(
        PREC.postfix,
        seq(field("value", $._expression), ".", field("field", choice($.identifier, $.type_identifier, $.integer))),
      ),
    index_expression: ($) => prec(PREC.postfix, seq(field("value", $._expression), "[", field("index", $._expression), "]")),
    slice_expression: ($) =>
      prec(
        PREC.postfix,
        seq(
          field("value", $._expression),
          "[",
          optional(field("start", $._expression)),
          choice("..", "..="),
          optional(field("end", $._expression)),
          "]",
        ),
      ),
    try_expression: ($) => prec(PREC.postfix, seq($._expression, "?")),
    super_call: ($) => seq("super", choice($.arguments, seq(".", field("method", $.identifier), $.arguments))),

    arguments: ($) => seq("(", commaSep(choice($._expression, $.named_argument)), ")"),
    named_argument: ($) => seq(field("name", $.identifier), ":", field("value", $._expression)),

    cast_expression: ($) =>
      prec.left(PREC.cast, seq(field("value", $._expression), field("operator", choice("as", "is")), field("type", $.type_identifier))),

    unary_expression: ($) => prec(PREC.unary, seq(field("operator", choice("-", "!", "await")), $._expression)),

    binary_expression: ($) =>
      choice(
        ...[
          [PREC.or, "||"],
          [PREC.and, "&&"],
          [PREC.equality, choice("==", "!=")],
          [PREC.compare, choice("<", "<=", ">", ">=")],
          [PREC.additive, choice("+", "-")],
          [PREC.multiplicative, choice("*", "/", "%")],
        ].map(([p, op]) =>
          prec.left(p, seq(field("left", $._expression), field("operator", op), field("right", $._expression))),
        ),
      ),

    identifier: (_) => /[a-z_][A-Za-z0-9_]*|[A-Z][A-Za-z0-9_]*/,
  },
});
