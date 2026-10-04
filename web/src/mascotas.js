// Las mascotas de MiyuLaTeX, con los mismos dibujos y ritmos que src/mascot.rs.
export function createMascots(canvas, portraits, initialColors, onStatus) {
  const WIDTH = 12;
  const MARGIN = 14;
  const SLEEP_AFTER = 60;
  const TYPING_FOR = 1.2;
  const SPEED = 28;
  const JUMP_TIME = 0.45;
  const JUMP_HEIGHT = 10;
  const ALARM_TIME = 1.4;
  const SHAKE_TIME = 0.4;
  const BLOOM_TIME = 8;
  const HOP_DELAY = 0.15;

  const SIT = [
    "X.....X.....",
    "XX...XX.....",
    "XXXXXXX...X.",
    "XoXXXoX...X.",
    "XXXXXXX..X..",
    "XXXXXXXXX...",
    "XXXXXXX.....",
    "X.X.X.X.....",
  ];
  const WAG = [
    "X.....X.....",
    "XX...XX.....",
    "XXXXXXX.....",
    "XoXXXoX....X",
    "XXXXXXX...X.",
    "XXXXXXXXXX..",
    "XXXXXXX.....",
    "X.X.X.X.....",
  ];
  const STEP = [
    "X.....X.....",
    "XX...XX.....",
    "XXXXXXX.....",
    "XoXXXoX....X",
    "XXXXXXX...X.",
    "XXXXXXXXXX..",
    "XXXXXXX.....",
    ".X.X.X......",
  ];
  const SLEEP = [
    "............",
    "............",
    "X.....X.....",
    "XX...XX.....",
    "XXXXXXX.....",
    "XXXXXXXXXX..",
    "XXXXXXXXXXX.",
    "XXXXXXXXXX..",
  ];
  const GROOM = [
    "X.....X.....",
    "XX...XX.....",
    "XXXXXXX.....",
    "XXXXXXX.....",
    "XXXXXXX...X.",
    "XXXXXXXXXX..",
    "XXXXXXX.....",
    "..X.X.X.....",
  ];
  const STRETCH = [
    [
      "..........X.",
      "..........X.",
      "........XXX.",
      "X.....XXXXX.",
      "XX...XXXXXX.",
      "XXXXXXXXX.X.",
      "XoXXXoX.X.X.",
      "XXXXXXX.X.X.",
    ],
    [
      "............",
      "...........X",
      "........XXX.",
      "X.....XXXXX.",
      "XX...XXXXXX.",
      "XXXXXXXXX.X.",
      "XoXXXoX.X.X.",
      "XXXXXXX.X.X.",
    ],
  ];
  const HEART = ["!.!", "!!!", ".!."];
  const EXCLAIM = ["!", "!", "!", ".", "!"];
  const ZETA = ["###", ".#.", "###"];
  const DOTS = [["....."], ["#...."], ["#.#.."], ["#.#.#"]];
  const PAW = ["X"];
  const BALL = ["**", "**"];
  const LAPTOP = [
    ["#.....", "#*....", ".#*...", ".#....", "..####"],
    ["#*....", "#.....", ".#*.XX", ".#....", "..####"],
  ];
  const CRAB = {
    still: [
      "...........",
      "XX.......XX",
      ".X.XXXXX.X.",
      ".XXXoXoXXX.",
      "..XXXXXXX..",
      "..X.X.X.X..",
    ],
    step: [
      "...........",
      "XX.......XX",
      ".X.XXXXX.X.",
      ".XXXoXoXXX.",
      "..XXXXXXX..",
      ".X..X.X..X.",
    ],
    happy: [
      "X.X.....X.X",
      ".X.......X.",
      ".X.XXXXX.X.",
      ".XXXoXoXXX.",
      "..XXXXXXX..",
      "..X.X.X.X..",
    ],
    sleep: [
      "...........",
      "...........",
      "...........",
      "XX.XXXXX.XX",
      ".XXXXXXXXX.",
      "..XXXXXXX..",
    ],
  };
  const DOG = {
    still: [
      "..X.X.......",
      ".wwXX.....X.",
      ".XoXX.....X.",
      "wwwXXXXXXXX.",
      "wwwXXXXXXXX.",
      ".w.XXXXXXXX.",
      "...w.w..w.w.",
      "...w.w..w.w.",
    ],
    step: [
      "..X.X.......",
      ".wwXX.....X.",
      ".XoXX.....X.",
      "wwwXXXXXXXX.",
      "wwwXXXXXXXX.",
      ".w.XXXXXXXX.",
      "...w.w..w.w.",
      "..w..w..w..w",
    ],
    happy: [
      "..X.X.......",
      ".wwXX......X",
      ".XoXX.....X.",
      "wwwXXXXXXXX.",
      "wwwXXXXXXXX.",
      ".w.XXXXXXXX.",
      "...w.w..w.w.",
      "...w.w..w.w.",
    ],
    sleep: [
      "............",
      "............",
      "............",
      "............",
      "..X.X.......",
      ".XXXXXXXXXX.",
      "wwwXXXXXXXXX",
      "wwwwwXXXXXww",
    ],
  };
  const BARK = [".!", "!.", ".!"];
  const JASMINE_BUDS = [
    "...w........",
    "...w....w...",
    "...g....w...",
    ".gg.g..g....",
    "....g.g.....",
    ".w...gg.....",
    ".wgg.gg.....",
    "....gg..w...",
    ".....g..wg..",
    ".....gggg...",
  ];
  const JASMINE_OPEN = [
    "...w........",
    "..w*w...w...",
    "...w...w*w..",
    ".gg.g..gw...",
    ".w..g.g.....",
    "w*w..gg.....",
    ".wgg.gg.w...",
    "....gg.w*w..",
    ".....g..wg..",
    ".....gggg...",
  ];
  const JASMINE_SWAYS = 5;
  const JASMINE_POT = [
    "..########..",
    "...######...",
    "...######...",
    "....####....",
  ];
  const SCENT = ["*"];

  // Tema miyu-noche.
  let C = initialColors;

  const g = canvas.getContext("2d");
  const motion = matchMedia("(prefers-reduced-motion: reduce)");
  let reduced = motion.matches;
  let frameId = 0;
  let compileTimer;
  let visible = false;
  const removers = [];
  const listen = (target, type, handler, options) => {
    target.addEventListener(type, handler, options);
    removers.push(() => target.removeEventListener(type, handler, options));
  };

  const random = (s) => {
    let x = s.v;
    x ^= x << 13;
    x >>>= 0;
    x ^= x >>> 17;
    x ^= x << 5;
    x >>>= 0;
    s.v = x;
    return (x >>> 8) / (1 << 24);
  };
  const catSeed = { v: 0x9e3779b9 >>> 0 };
  const rest = (s) => 4 + 8 * random(s);

  const cat = {
    at: 0.9,
    pose: "sit",
    to: 0,
    right: false,
    until: 6,
    active: 0,
    typed: -1e9,
    jumped: -1e9,
    alarmed: -1e9,
    busy: false,
    bloomed: -1e9,
    sleepy: false,
  };
  const friend = (kind, at, until, s) => ({
    kind,
    at,
    right: false,
    pose: "idle",
    to: 0,
    until,
    hopped: -1e9,
    seed: { v: s >>> 0 },
  });
  const friends = [
    friend("crab", 0.75, 3, 0x51f15eed),
    friend("dog", 0.15, 5, 0x0d06cafe),
  ];
  const shown = { cat: true, crab: true, dog: true, jasmine: true };

  let pixel = 3;
  let W = 0,
    H = 0,
    floor = 0,
    span = 1,
    last = 0;
  const hits = [];

  function resize() {
    const dpr = window.devicePixelRatio || 1;
    pixel = canvas.clientWidth < 640 ? 3 : 4;
    W = canvas.clientWidth;
    H = canvas.clientHeight;
    canvas.width = Math.round(W * dpr);
    canvas.height = Math.round(H * dpr);
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    floor = H;
    span = Math.max(1, W - WIDTH * pixel - 2 * MARGIN);
  }
  const observer = new ResizeObserver(resize);
  observer.observe(canvas);

  function paint(ox, oy, sprite, off, mirror, ink) {
    for (let row = 0; row < sprite.length; row++) {
      const line = sprite[row];
      for (let col = 0; col < line.length; col++) {
        const color = ink(line[col]);
        if (!color) continue;
        const left = mirror ? WIDTH - (col + 1) - off[0] : col + off[0];
        g.fillStyle = color;
        g.fillRect(
          ox + left * pixel,
          oy + (row + off[1]) * pixel,
          pixel,
          pixel,
        );
      }
    }
  }
  const inks = (body, blink) => (c) =>
    ({
      X: body,
      o: blink ? body : null,
      "#": C.muted,
      "*": C.accent,
      "!": C.error,
      w: C.fg,
      g: C.success,
    })[c] || null;

  function advanceCat(now, step, busy, typing) {
    if (now - cat.active > SLEEP_AFTER || cat.sleepy) {
      cat.pose = "sleep";
      return;
    }
    switch (cat.pose) {
      case "sleep":
        cat.pose = "sit";
        cat.until = now + 3;
        return;
      case "sit":
        if (now >= cat.until && !busy && !typing) {
          const choice = random(catSeed);
          if (choice < 0.45) {
            cat.to = random(catSeed);
            cat.right = cat.to > cat.at;
            cat.pose = "walk";
            cat.until = now;
          } else if (choice < 0.6) {
            cat.pose = "groom";
            cat.until = now + 2.5;
          } else if (choice < 0.75) {
            cat.pose = "play";
            cat.until = now + 4;
          } else if (choice < 0.85) {
            cat.pose = "stretch";
            cat.until = now + 2;
          } else cat.until = now + rest(catSeed);
        }
        return;
    }
    if (busy || typing) {
      cat.pose = "sit";
      cat.until = now + 3;
      return;
    }
    if (cat.pose === "walk") {
      if (Math.abs(cat.to - cat.at) <= step) {
        cat.at = cat.to;
        cat.pose = "sit";
        cat.until = now + rest(catSeed);
      } else cat.at += Math.sign(cat.to - cat.at) * step;
    } else if (now >= cat.until) {
      cat.pose = "sit";
      cat.until = now + rest(catSeed);
    }
  }

  function reach(p, to, step) {
    if (Math.abs(to - p.at) <= step) {
      p.at = to;
      return true;
    }
    p.right = to > p.at;
    p.at += Math.sign(to - p.at) * step;
    return false;
  }
  function advanceFriend(p, now, step, beside) {
    switch (p.pose) {
      case "idle":
        if (now >= p.until) {
          const choice = random(p.seed);
          if (choice < 0.45) {
            p.pose = "walk";
            p.to = random(p.seed);
          } else if (choice < 0.75) {
            p.pose = "visit";
            p.until = now + 15;
          } else p.until = now + rest(p.seed);
        }
        break;
      case "walk":
        if (reach(p, p.to, step)) {
          p.pose = "idle";
          p.until = now + rest(p.seed);
        }
        break;
      case "visit":
        if (reach(p, beside, step)) {
          p.pose = "greet";
          p.until = now + 1.6;
          return true;
        }
        if (now >= p.until) {
          p.pose = "idle";
          p.until = now + rest(p.seed);
        }
        break;
      case "greet":
        if (now >= p.until) {
          p.pose = "idle";
          p.until = now + rest(p.seed);
        }
        break;
    }
    return false;
  }

  function frame(t) {
    const now = t / 1000;
    const elapsed = Math.min(Math.max(now - last, 0), 0.25);
    last = now;
    const busy = cat.busy;
    const typing = now - cat.typed < TYPING_FOR;
    const fr = (rate) => Math.floor(now * rate);
    g.clearRect(0, 0, W, H);
    hits.length = 0;

    advanceCat(now, reduced ? 0 : (elapsed * SPEED) / span, busy, typing);

    const jump = (now - cat.jumped) / JUMP_TIME;
    const jumping = jump >= 0 && jump < 1;
    const lift = jumping
      ? Math.round((4 * jump * (1 - jump) * JUMP_HEIGHT * pixel) / 2)
      : 0;
    const alarmed = now - cat.alarmed < ALARM_TIME;
    const shaking = now - cat.alarmed < SHAKE_TIME;
    const shake = shaking && fr(14) % 2 === 0 ? pixel : 0;
    const size = [WIDTH * pixel, 8 * pixel];
    const ox = Math.round(MARGIN + cat.at * span) + shake;
    const oy = floor - size[1] - lift;

    const around = [];
    let blink = false;
    let sprite;
    switch (cat.pose) {
      case "sleep": {
        const s = fr(1) % 3;
        if (s >= 1) around.push([ZETA, 8, -3]);
        if (s === 2) around.push([ZETA, 12, -7]);
        sprite = SLEEP;
        break;
      }
      case "walk":
        sprite = fr(6) % 2 === 0 ? SIT : STEP;
        break;
      case "groom":
        around.push([PAW, -1, fr(4) % 2 === 0 ? 4 : 3]);
        sprite = GROOM;
        break;
      case "play": {
        const s = fr(6) % 4;
        around.push([BALL, [-3, -3, -5, -4][s], s === 2 ? 5 : 6]);
        if (s === 1) around.push([PAW, -1, 6]);
        sprite = s % 2 === 0 ? SIT : WAG;
        break;
      }
      case "stretch":
        sprite = STRETCH[fr(2) % 2];
        break;
      default:
        if (typing && !jumping) {
          around.push([LAPTOP[fr(8) % 2], -6, 3]);
          blink = fr(8) % 30 === 0;
          sprite = SIT;
        } else {
          const tick = fr(4);
          if (busy) around.push([DOTS[tick % 4], 1, -2]);
          blink = tick % 15 === 0;
          const wagging = jumping || busy || tick % 24 < 6;
          sprite = wagging && tick % 2 === 1 ? WAG : SIT;
        }
    }
    if (jumping) around.push([HEART, 2, -5]);
    else if (alarmed) around.push([EXCLAIM, 3, -7]);

    const asleep = cat.pose === "sleep";
    if (shown.jasmine) {
      const jh = (JASMINE_BUDS.length + JASMINE_POT.length) * pixel;
      const jx = MARGIN,
        jy = floor - jh;
      hits.push({
        x: jx,
        y: jy,
        w: 12 * pixel,
        h: jh,
        hit: () => {
          cat.bloomed = now;
          cat.active = now;
        },
      });
      const open = asleep || now - cat.bloomed < BLOOM_TIME;
      const ink = inks(C.success, false);
      const sway = fr(2) % 9 === 0 ? 1 : 0;
      const branches = open ? JASMINE_OPEN : JASMINE_BUDS;
      paint(jx, jy, branches.slice(0, JASMINE_SWAYS), [sway, 0], false, ink);
      paint(
        jx,
        jy,
        branches.slice(JASMINE_SWAYS),
        [0, JASMINE_SWAYS],
        false,
        ink,
      );
      paint(jx, jy, JASMINE_POT, [0, JASMINE_BUDS.length], false, ink);
      if (asleep) {
        const rise = fr(2) % 4;
        paint(jx, jy, SCENT, [2 + (rise % 2), -1 - 2 * rise], false, ink);
        paint(
          jx,
          jy,
          SCENT,
          [8 - (rise % 2), -2 * ((rise + 2) % 4)],
          false,
          ink,
        );
      }
    }

    friends.forEach((p) => {
      if (!shown[p.kind]) return;
      const look = p.kind === "dog" ? DOG : CRAB;
      const width = look.still[0].length;
      const gap = (px) => ((px + 2) * pixel) / span;
      const after = cat.at + gap(WIDTH);
      const before = cat.at - gap(width);
      const beside = Math.min(
        1,
        Math.max(
          0,
          (p.at > cat.at && after <= 1) || before < 0 ? after : before,
        ),
      );
      if (
        !reduced &&
        !asleep &&
        advanceFriend(p, now, (1.3 * elapsed * SPEED) / span, beside)
      )
        cat.jumped = now;
      if (p.pose === "greet") p.right = cat.at > p.at;
      const hop =
        (now - Math.max(p.hopped, cat.jumped + HOP_DELAY)) / JUMP_TIME;
      const hopping = hop >= 0 && hop < 1;
      const plift = hopping
        ? Math.round((4 * hop * (1 - hop) * JUMP_HEIGHT * pixel) / 2)
        : 0;
      const px = Math.round(MARGIN + p.at * span);
      const py = floor - look.still.length * pixel - plift;
      hits.push({
        x: px,
        y: py,
        w: width * pixel,
        h: look.still.length * pixel,
        hit: () => {
          if (!hopping) {
            p.hopped = now;
            cat.active = now;
          }
        },
      });
      const tick = fr(4);
      const dog = p.kind === "dog";
      const [every, shift] = dog ? [22, 3] : [28, 9];
      let s;
      if (asleep) s = look.sleep;
      else if (hopping) s = look.happy;
      else if (p.pose === "walk" || p.pose === "visit")
        s = fr(8) % 2 === 0 ? look.still : look.step;
      else {
        const happy = p.pose === "greet" || (tick + shift) % every < 6;
        s = happy && tick % 2 === 0 ? look.happy : look.still;
      }
      const ink = inks(dog ? C.muted : C.secondary, (tick + shift) % 17 === 0);
      const mirror = dog && p.right;
      paint(px, py, s, [0, 0], mirror, ink);
      if (dog && alarmed && tick % 2 === 0)
        paint(px, py, BARK, [-3, 2], mirror, ink);
    });

    if (shown.cat) {
      hits.push({
        x: ox,
        y: oy,
        w: size[0],
        h: size[1],
        hit: () => {
          if (!jumping) {
            cat.jumped = now;
            cat.active = now;
          }
        },
      });
      const ink = inks(C.primary, blink);
      paint(ox, oy, sprite, [0, 0], cat.right, ink);
      around.forEach(([s, c, r]) => paint(ox, oy, s, [c, r], cat.right, ink));
    }
    if (!reduced && visible && !document.hidden)
      frameId = requestAnimationFrame(frame);
  }

  function poke() {
    cat.active = performance.now() / 1000;
  }
  function drawOnce() {
    if (reduced) frame(performance.now());
  }
  listen(canvas.closest("section"), "keydown", () => {
    cat.typed = performance.now() / 1000;
    cat.sleepy = false;
    poke();
    drawOnce();
  });
  listen(canvas, "click", (event) => {
    const rect = canvas.getBoundingClientRect();
    const x = event.clientX - rect.left,
      y = event.clientY - rect.top;
    const hit = hits.find(
      (item) =>
        x >= item.x &&
        x < item.x + item.w &&
        y >= item.y - 2 &&
        y < item.y + item.h,
    );
    if (hit) {
      hit.hit();
      drawOnce();
    }
  });

  const now = () => performance.now() / 1000;
  function run(action) {
    if (action === "sleep") {
      clearTimeout(compileTimer);
      cat.busy = false;
      cat.sleepy = !cat.sleepy;
      poke();
      drawOnce();
      onStatus(
        cat.sleepy
          ? "Miyu duerme y el jazmín florece."
          : "Miyu vuelve a pasear.",
        cat.sleepy,
        false,
      );
      return;
    }
    clearTimeout(compileTimer);
    cat.sleepy = false;
    cat.busy = true;
    poke();
    drawOnce();
    onStatus("Miyu espera mientras compila…", false, true);
    compileTimer = setTimeout(() => {
      cat.busy = false;
      if (action === "success") {
        cat.jumped = cat.bloomed = now();
        onStatus(
          "Compilación lista. Miyu, Coco y Congo celebran.",
          false,
          false,
        );
      } else {
        cat.alarmed = now();
        onStatus("Hay un error. Miyu se asusta y Congo ladra.", false, false);
      }
      poke();
      drawOnce();
    }, 1800);
  }

  // Sprites quietos para las fichas.
  function drawPortraits() {
    portraits.forEach((c) => {
      const sprites = {
        cat: [SIT, C.primary],
        crab: [CRAB.happy, C.secondary],
        dog: [DOG.happy, C.muted],
        jasmine: [JASMINE_OPEN.concat(JASMINE_POT), C.success],
      };
      const [s, body] = sprites[c.dataset.sprite];
      const p = 5,
        w = s[0].length,
        h = s.length;
      c.width = w * p;
      c.height = h * p;
      const x = c.getContext("2d");
      const ink = inks(body, false);
      s.forEach((line, row) =>
        [...line].forEach((ch, col) => {
          const color = ink(ch);
          if (color) {
            x.fillStyle = color;
            x.fillRect(col * p, row * p, p, p);
          }
        }),
      );
    });
  }
  function redraw() {
    cancelAnimationFrame(frameId);
    frame(performance.now());
  }
  listen(motion, "change", (event) => {
    reduced = event.matches;
    redraw();
  });
  listen(document, "visibilitychange", redraw);
  const visibility = new IntersectionObserver((entries) => {
    visible = entries[0].isIntersecting;
    redraw();
  });
  visibility.observe(canvas);
  resize();
  drawPortraits();
  redraw();
  return {
    run,
    wake() {
      cat.sleepy = false;
      cat.typed = now();
      poke();
      onStatus("Miyu teclea contigo.", false, cat.busy);
      drawOnce();
    },
    setColors(colors) {
      C = colors;
      drawPortraits();
      redraw();
    },
    destroy() {
      cancelAnimationFrame(frameId);
      clearTimeout(compileTimer);
      observer.disconnect();
      visibility.disconnect();
      removers.forEach((remove) => remove());
    },
  };
}
