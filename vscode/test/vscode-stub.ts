// Just enough of the `vscode` module for comments.ts.
export class Position {
  constructor(public line: number, public character: number) {}
}
export class Range {
  start: Position;
  end: Position;
  constructor(a: number | Position, b: number | Position, c?: number, d?: number) {
    this.start = typeof a === 'number' ? new Position(a, b as number) : a;
    this.end = typeof a === 'number' ? new Position(c!, d!) : (b as Position);
  }
}
export function doc(text: string, languageId: string) {
  const lines = text.split('\n');
  const starts = lines.reduce<number[]>((acc, l, i) => (acc.push(i ? acc[i - 1]! + lines[i - 1]!.length + 1 : 0), acc), []);
  return {
    languageId,
    lineCount: lines.length,
    lineAt: (n: number) => ({ text: lines[n]! }),
    offsetAt: (p: Position) => starts[p.line]! + p.character,
    positionAt: (o: number) => {
      let line = starts.findIndex((s, i) => o >= s && (i === starts.length - 1 || o < starts[i + 1]!));
      if (line < 0) line = lines.length - 1;
      return new Position(line, o - starts[line]!);
    },
    getText: (r: Range) => text.slice(starts[r.start.line]! + r.start.character, starts[r.end.line]! + r.end.character),
  };
}
