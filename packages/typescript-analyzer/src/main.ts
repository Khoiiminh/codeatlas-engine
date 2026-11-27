// stdio entry point, spawned by crates/adapters/codeatlas-semantic-typescript.
// Protocol messages go to stdout; diagnostics go to stderr only.
// Resolution logic (src/analysis) and message types (src/protocol) arrive with ENG-03.

process.stderr.write("typescript-analyzer: not implemented yet\n");
process.exitCode = 1;
