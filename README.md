an extensible prat parser written for multi threaded reads.

# Progress
a basic parser is already implemented for expressions.
it is fairly extensible capable of handeling syntax extentions at runtime.

error reporting is fairly solid with line numbers included.
there are a few places where it could be improved but for a first draft this is very solid.

the code is thread safe with costs being fairly acceptble (some edge cases of weird macro expantions are slightly more expensive than needed)

# Design 

we are choosing C style text macros. this is for a few reasons:

1. they are simpler in concept to an AST and do not require any languge specific knowledge
2. they do not force me to expose the AST to the PL
3. they allow completly external tools to be used in the macro (like just reading a file)

ideally a similarly simple API could be applied for backend code.
as an example having just compile time excutions solves the ABI problem. but it adds problems of its own around having multiple passes.


