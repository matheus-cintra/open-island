type Bitmap = readonly string[];

interface AgentSprite {
  readonly frames: readonly [Bitmap, Bitmap];
  readonly glyph: Bitmap;
}

const ALIEN_WIDTH = 11;
const GLYPH_GAP = 1;

export const SPRITE_WIDTH = 18;
export const SPRITE_HEIGHT = 8;

const SPRITES: Record<string, AgentSprite> = {
  claude: {
    frames: [
      [
        "..##...##..",
        "..##...##..",
        "###########",
        "##.#####.##",
        "###########",
        "###########",
        "###########",
        "###.###.###",
      ],
      [
        ".##.....##.",
        "..##...##..",
        "###########",
        "##.#####.##",
        "###########",
        "###########",
        "###########",
        "####...####",
      ],
    ],
    glyph: [".####.", "##..##", "....##", "...##.", "..##..", "..##..", "......", "..##.."],
  },
  codex: {
    frames: [
      [
        "....###....",
        "...#####...",
        "..#######..",
        ".##.###.##.",
        "###########",
        "#.#######.#",
        "#.#.....#.#",
        "...##.##...",
      ],
      [
        "....###....",
        "...#####...",
        "..#######..",
        ".##.###.##.",
        "###########",
        "#.#######.#",
        "..#.....#..",
        ".##.....##.",
      ],
    ],
    glyph: ["..##..", "..##..", "..##..", "..##..", "..##..", "..##..", "..##..", "..##.."],
  },
  opencode: {
    frames: [
      [
        "...#####...",
        ".#########.",
        "###########",
        "###.###.###",
        "###########",
        "..##.#.##..",
        ".##..#..##.",
        "##.......##",
      ],
      [
        "...#####...",
        ".#########.",
        "###########",
        "###.###.###",
        "###########",
        "...##.##...",
        "..##.#.##..",
        ".##.....##.",
      ],
    ],
    glyph: ["......", "..##..", "..##..", "######", "######", "..##..", "..##..", "......"],
  },
};

const UNKNOWN: AgentSprite = {
  frames: [
    [
      "..#.....#..",
      "...#...#...",
      "..#######..",
      ".#########.",
      "###########",
      "###########",
      "..##...##..",
      ".#.......#.",
    ],
    [
      "..#.....#..",
      "...#...#...",
      "..#######..",
      ".#########.",
      "###########",
      "###########",
      ".##.....##.",
      "#..#...#..#",
    ],
  ],
  glyph: ["......", "......", "..##..", ".####.", ".####.", "..##..", "......", "......"],
};

const SVG_NS = "http://www.w3.org/2000/svg";

function pathData(bitmap: Bitmap, offsetX: number): string {
  let data = "";
  bitmap.forEach((line, y) => {
    let x = 0;
    while (x < line.length) {
      if (line[x] !== "#") {
        x += 1;
        continue;
      }
      let run = 1;
      while (line[x + run] === "#") run += 1;
      data += `M${offsetX + x} ${y}h${run}v1h${-run}z`;
      x += run;
    }
  });
  return data;
}

function framePath(className: string, data: string): SVGPathElement {
  const path = document.createElementNS(SVG_NS, "path");
  path.setAttribute("class", className);
  path.setAttribute("d", data);
  return path;
}

export function spriteAgent(agent: string): string {
  return agent in SPRITES ? agent : "unknown";
}

export function createSprite(agent: string): SVGSVGElement {
  const sprite = SPRITES[agent] ?? UNKNOWN;
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("class", "island-sprite");
  svg.setAttribute("viewBox", `0 0 ${SPRITE_WIDTH} ${SPRITE_HEIGHT}`);
  svg.setAttribute("shape-rendering", "crispEdges");
  svg.setAttribute("aria-hidden", "true");
  svg.dataset.agent = spriteAgent(agent);
  svg.append(
    framePath("sprite-glyph", pathData(sprite.glyph, ALIEN_WIDTH + GLYPH_GAP)),
    framePath("sprite-frame sprite-frame-a", pathData(sprite.frames[0], 0)),
    framePath("sprite-frame sprite-frame-b", pathData(sprite.frames[1], 0)),
  );
  return svg;
}
