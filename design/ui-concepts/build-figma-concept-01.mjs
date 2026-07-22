import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const WIDTH = 4580;
const HEIGHT = 4550;
const SCREEN_W = 1440;
const SCREEN_H = 960;
const VIEW_Y = 58;
const VIEW_H = SCREEN_H - VIEW_Y;
const out = [];

const colors = {
  app: "#edf3fa",
  panel: "#f8fbff",
  raised: "#ffffff",
  inset: "#e8eef7",
  ink: "#0f172a",
  ink2: "#334155",
  ink3: "#64748b",
  ink4: "#94a3b8",
  line: "#d6dee9",
  line2: "#b8c5d6",
  blue: "#2563eb",
  blueDeep: "#1d4ed8",
  blueSoft: "#e8f0ff",
  green: "#059669",
  greenSoft: "#e5f7f1",
  orange: "#d97706",
  orangeSoft: "#fff4df",
  red: "#dc2626",
  redSoft: "#feecec",
  violet: "#6d5bd0",
  violetSoft: "#f0edff",
};

function esc(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function rect(x, y, w, h, fill = colors.raised, stroke = "none", radius = 0, strokeWidth = 1, opacity = 1) {
  out.push(
    `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="${radius}" fill="${fill}" stroke="${stroke}" stroke-width="${strokeWidth}" opacity="${opacity}"/>`,
  );
}

function line(x1, y1, x2, y2, stroke = colors.line2, strokeWidth = 1, dash = "") {
  out.push(
    `<line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" stroke="${stroke}" stroke-width="${strokeWidth}"${dash ? ` stroke-dasharray="${dash}"` : ""}/>`
  );
}

function path(d, stroke = colors.line2, strokeWidth = 1, dash = "", marker = "") {
  out.push(
    `<path d="${d}" fill="none" stroke="${stroke}" stroke-width="${strokeWidth}" stroke-linecap="round" stroke-linejoin="round"${dash ? ` stroke-dasharray="${dash}"` : ""}${marker ? ` marker-end="url(#${marker})"` : ""}/>`
  );
}

function circle(cx, cy, r, fill = colors.raised, stroke = "none", strokeWidth = 1) {
  out.push(`<circle cx="${cx}" cy="${cy}" r="${r}" fill="${fill}" stroke="${stroke}" stroke-width="${strokeWidth}"/>`);
}

function text(x, y, value, size = 14, weight = 400, fill = colors.ink2, anchor = "start", mono = false) {
  out.push(
    `<text x="${x}" y="${y}" font-size="${size}" font-weight="${weight}" fill="${fill}" text-anchor="${anchor}" class="${mono ? "mono" : "sans"}">${esc(value)}</text>`,
  );
}

function multiline(x, y, lines, size = 14, weight = 400, fill = colors.ink2, lineHeight = 21, mono = false) {
  lines.forEach((value, index) => text(x, y + index * lineHeight, value, size, weight, fill, "start", mono));
}

function pill(x, y, label, options = {}) {
  const size = options.size ?? 12;
  const padX = options.padX ?? 10;
  const width = options.width ?? Math.max(42, label.length * (options.mono ? 7.4 : 7.1) + padX * 2);
  const height = options.height ?? 26;
  rect(x, y, width, height, options.fill ?? colors.inset, options.stroke ?? "none", options.radius ?? 6, 1);
  text(x + width / 2, y + height / 2 + size * 0.36, label, size, options.weight ?? 600, options.color ?? colors.ink2, "middle", options.mono ?? false);
  return width;
}

function button(x, y, w, label, primary = false, disabled = false) {
  const fill = disabled ? "#e2e8f0" : primary ? colors.blue : colors.raised;
  const stroke = disabled ? "#e2e8f0" : primary ? colors.blue : colors.line2;
  const ink = disabled ? colors.ink4 : primary ? "#ffffff" : colors.ink2;
  rect(x, y, w, 38, fill, stroke, 6, 1);
  text(x + w / 2, y + 24, label, 13, 650, ink, "middle");
}

function divider(x, y, w) {
  line(x, y, x + w, y, colors.line, 1);
}

function titleBlock(x, y, eyebrow, titleValue, bodyLines = []) {
  text(x, y, eyebrow, 11, 700, colors.blue);
  text(x, y + 30, titleValue, 22, 700, colors.ink);
  if (bodyLines.length) multiline(x, y + 57, bodyLines, 13, 400, colors.ink3, 19);
}

function statusDot(x, y, color) {
  circle(x, y, 4, color);
}

function topbar({ project = "meeting-overlay-assistant", code = "코드 최신", db = "DB 최신", search = true, setup = false } = {}) {
  rect(0, 0, SCREEN_W, 62, colors.raised);
  line(0, 61, SCREEN_W, 61, colors.line, 1);
  rect(20, 15, 32, 32, colors.blue, colors.blue, 6);
  rect(28, 22, 16, 3, "#ffffff", "none", 1);
  rect(28, 29, 16, 3, "#ffffff", "none", 1);
  rect(28, 36, 16, 3, "#ffffff", "none", 1);
  text(64, 37, "백엔드 비주얼 맵", 17, 700, colors.ink);
  rect(244, 12, 286, 38, colors.panel, colors.line, 6);
  text(260, 36, setup ? "프로젝트 연결" : project, 13, 600, setup ? colors.ink3 : colors.ink2);
  text(511, 36, "⌄", 16, 600, colors.ink3, "middle");
  if (!setup) {
    rect(546, 16, 102, 30, colors.greenSoft, "none", 15);
    statusDot(560, 31, colors.green);
    text(572, 35, code, 11, 600, colors.green);
    rect(656, 16, 88, 30, colors.greenSoft, "none", 15);
    statusDot(670, 31, colors.green);
    text(682, 35, db, 11, 600, colors.green);
  }
  if (search && !setup) {
    rect(928, 12, 356, 38, colors.panel, colors.line, 6);
    circle(948, 30, 6, "none", colors.ink4, 1.4);
    line(952, 35, 958, 41, colors.ink4, 1.4);
    text(968, 36, "API, 함수, 파일, 테이블, 컬럼 찾기", 12, 400, colors.ink3);
    pill(1233, 20, "Ctrl K", { width: 42, height: 22, size: 10, fill: colors.inset, color: colors.ink3 });
  }
  rect(1300, 14, 34, 34, colors.raised, colors.line, 6);
  text(1317, 36, "?", 14, 700, colors.ink3, "middle");
  rect(1344, 14, 34, 34, colors.raised, colors.line, 6);
  text(1361, 36, "⋯", 18, 700, colors.ink3, "middle");
}

const modes = [
  ["01", "전체 구조", "프로젝트를 빠르게 훑기"],
  ["02", "API가 닿는 코드", "진입점부터 데이터 접근까지"],
  ["03", "테이블 사용처", "읽기·쓰기와 FK 확인"],
  ["04", "변경 영향", "컬럼 변경 전 검토"],
];

function navRail(active = 0, options = {}) {
  rect(0, 62, 224, VIEW_H - 28, colors.panel);
  line(223, 62, 223, VIEW_H - 28 + 62, colors.line, 1);
  text(22, 92, "찾을 답", 12, 700, colors.ink3);
  modes.forEach(([num, label, sub], index) => {
    const y = 110 + index * 78;
    if (index === active) rect(12, y, 200, 64, colors.blueSoft, "#a9c5ff", 6, 1);
    text(28, y + 24, num, 11, 700, index === active ? colors.blue : colors.ink4);
    text(58, y + 24, label, 14, 650, index === active ? colors.blueDeep : colors.ink2);
    text(58, y + 45, sub, 10, 400, colors.ink3);
  });
  divider(18, 438, 188);
  text(22, 466, "보기", 11, 700, colors.ink3);
  pill(22, 482, options.view ?? "도메인", { width: 76, height: 28, fill: colors.raised, stroke: colors.line2, color: colors.ink2 });
  pill(104, 482, "계층", { width: 58, height: 28, fill: colors.raised, stroke: colors.line, color: colors.ink3 });
  text(22, 560, "소스", 11, 700, colors.ink3);
  text(22, 584, "코드 · 연결됨", 11, 600, colors.green);
  text(22, 605, "DB · 연결됨", 11, 600, colors.green);
  text(22, VIEW_H - 8, "소스 및 인덱싱 관리  →", 11, 600, colors.blue);
}

function statusbar(left = "스냅샷 · 방금 전", center = "확정과 후보를 분리해 표시", right = "코드 287 · DB 20") {
  const y = VIEW_H - 28;
  rect(0, y, SCREEN_W, 28, "#f4f7fb");
  line(0, y, SCREEN_W, y, colors.line, 1);
  text(18, y + 19, left, 10, 500, colors.ink3);
  text(SCREEN_W / 2, y + 19, center, 10, 500, colors.ink3, "middle");
  text(SCREEN_W - 18, y + 19, right, 10, 600, colors.ink2, "end");
}

function screen(x, y, titleValue, subtitle, render) {
  out.push(`<g transform="translate(${x} ${y})">`);
  rect(0, 0, SCREEN_W, SCREEN_H, colors.raised, colors.line2, 8, 1);
  text(22, 35, titleValue, 17, 700, colors.ink);
  text(SCREEN_W - 22, 35, subtitle, 11, 500, colors.ink3, "end");
  line(0, VIEW_Y - 1, SCREEN_W, VIEW_Y - 1, colors.line, 1);
  out.push(`<g transform="translate(0 ${VIEW_Y})">`);
  rect(0, 0, SCREEN_W, VIEW_H, colors.app);
  render();
  out.push("</g></g>");
}

function smallCard(x, y, w, h, titleValue, lines = [], tone = "neutral") {
  const toneMap = {
    neutral: [colors.raised, colors.line2, colors.ink2],
    blue: [colors.blueSoft, "#a9c5ff", colors.blueDeep],
    green: [colors.greenSoft, "#9bd9c5", colors.green],
    orange: [colors.orangeSoft, "#efc888", colors.orange],
    red: [colors.redSoft, "#f4a8a8", colors.red],
    violet: [colors.violetSoft, "#c9c1f3", colors.violet],
  };
  const [fill, stroke, ink] = toneMap[tone];
  rect(x, y, w, h, fill, stroke, 6, 1);
  text(x + 14, y + 24, titleValue, 13, 650, ink);
  if (lines.length) multiline(x + 14, y + 48, lines, 11, 400, colors.ink3, 17);
}

function sourceRow(x, y, number, titleValue, state, detail, tone = "green") {
  const color = tone === "orange" ? colors.orange : tone === "blue" ? colors.blue : tone === "red" ? colors.red : colors.green;
  circle(x + 17, y + 17, 17, color);
  text(x + 17, y + 22, number, 11, 700, "#ffffff", "middle");
  text(x + 48, y + 15, titleValue, 14, 650, colors.ink);
  text(x + 48, y + 36, detail, 11, 400, colors.ink3);
  pill(x + 500, y + 2, state, { width: 84, height: 28, fill: tone === "orange" ? colors.orangeSoft : tone === "blue" ? colors.blueSoft : tone === "red" ? colors.redSoft : colors.greenSoft, color });
}

function domainCard(x, y, w, titleValue, count, api, code, db, selected = false, faded = false) {
  rect(x, y, w, 208, selected ? "#f7faff" : colors.raised, selected ? colors.blue : colors.line2, 7, selected ? 2 : 1, faded ? 0.38 : 1);
  circle(x + 22, y + 24, 11, selected ? colors.blue : colors.inset, selected ? colors.blue : colors.line2, 1);
  text(x + 43, y + 29, titleValue, 15, 700, colors.ink);
  pill(x + w - 48, y + 12, String(count), { width: 34, height: 28, fill: colors.panel, stroke: colors.line, color: colors.ink3 });
  divider(x + 12, y + 47, w - 24);
  text(x + 14, y + 72, "API", 11, 600, colors.ink3);
  let px = x + 58;
  api.forEach((value) => { px += pill(px, y + 56, value, { height: 27, mono: true, fill: colors.blueSoft, color: colors.blueDeep }) + 6; });
  text(x + 14, y + 113, "Code", 11, 600, colors.ink3);
  px = x + 58;
  code.forEach((value) => { px += pill(px, y + 97, value, { height: 27, mono: true, fill: colors.violetSoft, color: colors.violet }) + 6; });
  text(x + 14, y + 154, "DB", 11, 600, colors.ink3);
  px = x + 58;
  db.forEach((value) => { px += pill(px, y + 138, value, { height: 27, mono: true, fill: colors.greenSoft, color: colors.green }) + 6; });
  if (selected) pill(x + w - 72, y - 12, "선택", { width: 58, height: 24, fill: colors.blue, color: "#ffffff" });
}

function resultRow(x, y, w, type, titleValue, meta, tone = "blue", selected = false) {
  const fill = selected ? colors.blueSoft : colors.raised;
  rect(x, y, w, 54, fill, selected ? "#a9c5ff" : colors.line, 5, 1);
  const toneFill = tone === "green" ? colors.greenSoft : tone === "violet" ? colors.violetSoft : colors.blueSoft;
  const toneInk = tone === "green" ? colors.green : tone === "violet" ? colors.violet : colors.blueDeep;
  pill(x + 12, y + 13, type, { width: 64, height: 28, fill: toneFill, color: toneInk, mono: true });
  text(x + 90, y + 24, titleValue, 13, 650, colors.ink, "start", true);
  text(x + 90, y + 43, meta, 10, 400, colors.ink3);
}

function evidenceTab() {
  rect(1398, 348, 42, 116, colors.raised, colors.line2, 6, 1);
  text(1419, 376, "근", 12, 700, colors.blue, "middle");
  text(1419, 397, "거", 12, 700, colors.blue, "middle");
  pill(1405, 418, "3", { width: 28, height: 24, fill: colors.blueSoft, color: colors.blue });
}

function drawSetup() {
  topbar({ setup: true, search: false });
  titleBlock(100, 120, "처음 한 번만", "어떤 프로젝트를 읽을까요?", ["코드부터 연결하면 전체 구조와 API 진입점을 바로 볼 수 있습니다."]);
  rect(100, 202, 1240, 584, colors.raised, colors.line2, 8, 1);
  rect(100, 202, 276, 584, colors.panel, "none", 8);
  text(128, 240, "연결 순서", 12, 700, colors.ink3);
  const steps = [
    ["1", "프로젝트", "현재 단계", colors.blue],
    ["2", "코드 읽기", "자동 진행", colors.ink4],
    ["3", "DB 연결", "선택 사항", colors.ink4],
    ["4", "답 찾기", "준비 완료", colors.ink4],
  ];
  steps.forEach(([num, label, state, color], index) => {
    const sy = 278 + index * 86;
    circle(148, sy, 16, index === 0 ? colors.blue : colors.raised, color, 1.5);
    text(148, sy + 5, num, 11, 700, index === 0 ? "#ffffff" : color, "middle");
    text(178, sy - 2, label, 14, 650, colors.ink2);
    text(178, sy + 19, state, 10, 400, colors.ink3);
    if (index < 3) line(148, sy + 18, 148, sy + 69, colors.line2, 1.5);
  });
  text(410, 246, "프로젝트 소스", 13, 700, colors.ink);
  rect(410, 270, 324, 40, colors.inset, colors.line, 6);
  rect(414, 274, 154, 32, colors.raised, colors.line2, 5);
  text(491, 295, "로컬 폴더", 12, 650, colors.blueDeep, "middle");
  text(646, 295, "GitHub URL", 12, 550, colors.ink3, "middle");
  text(410, 344, "프로젝트 폴더", 11, 650, colors.ink2);
  rect(410, 360, 750, 42, colors.panel, colors.line2, 6);
  text(426, 386, "D:/project/meeting-overlay-assistant", 12, 500, colors.ink2, "start", true);
  button(1172, 362, 118, "폴더 선택", false);
  smallCard(410, 434, 880, 92, "코드 소스 · 자동", ["프로젝트를 열면 API, 함수, 클래스, 파일을 읽습니다."], "blue");
  pill(1192, 448, "필수", { width: 68, height: 28, fill: colors.blueSoft, color: colors.blue });
  smallCard(410, 542, 880, 108, "DB 소스 · 선택", ["SQLite, PostgreSQL, MySQL 또는 DDL 파일을 나중에 연결할 수 있습니다.", "DB 없이도 코드 기반 답은 즉시 사용할 수 있습니다."], "neutral");
  pill(1176, 556, "나중에", { width: 84, height: 28, fill: colors.inset, color: colors.ink3 });
  button(410, 690, 166, "프로젝트 열기", true);
  text(594, 714, "열기 후 코드 분석이 자동으로 시작됩니다.", 11, 400, colors.ink3);
}

function drawIndexing() {
  topbar({ db: "DB 미연결" });
  titleBlock(84, 116, "분석 중에도 사용 가능", "프로젝트를 읽고 있습니다", ["끝날 때까지 기다리지 않아도 준비된 답부터 열 수 있습니다."]);
  rect(84, 200, 820, 550, colors.raised, colors.line2, 8, 1);
  text(112, 240, "분석 진행", 14, 700, colors.ink);
  sourceRow(112, 280, "1", "프로젝트 열기", "완료", "meeting-overlay-assistant", "green");
  line(129, 320, 129, 351, colors.green, 2);
  sourceRow(112, 356, "2", "코드 구조 읽기", "68%", "API 41 · 함수/클래스 196 · 파일 164", "blue");
  rect(160, 412, 584, 8, colors.inset, "none", 4);
  rect(160, 412, 397, 8, colors.blue, "none", 4);
  line(129, 396, 129, 461, colors.line2, 2);
  sourceRow(112, 466, "3", "관계 정리", "진행 중", "HANDLES/CALLS와 파일 위치를 확인", "orange");
  line(129, 506, 129, 557, colors.line2, 2);
  sourceRow(112, 562, "4", "DB 구조 읽기", "선택", "지금 건너뛰고 나중에 연결 가능", "blue");
  divider(112, 628, 764);
  text(112, 660, "현재까지 읽은 내용은 자동 저장됩니다.", 11, 500, colors.ink3);
  button(112, 684, 148, "백그라운드로", false);
  rect(936, 200, 420, 550, colors.panel, colors.line2, 8, 1);
  text(964, 240, "지금 가능한 답", 14, 700, colors.ink);
  pill(1248, 220, "2개 열림", { width: 86, height: 28, fill: colors.greenSoft, color: colors.green });
  smallCard(964, 272, 364, 108, "전체 구조", ["읽힌 API·코드·파일을 도메인별로 확인"], "blue");
  button(982, 326, 132, "지금 보기", true);
  smallCard(964, 396, 364, 108, "API 진입점", ["라우트 41개를 먼저 고를 수 있음"], "green");
  button(982, 450, 132, "API 보기", false);
  smallCard(964, 520, 364, 116, "변경 영향", ["DB 컬럼이 연결되면 자동으로 열립니다."], "neutral");
  button(982, 582, 132, "DB 연결", false);
  text(964, 686, "완료 알림은 상태바에서만 조용히 표시합니다.", 10, 400, colors.ink3);
  statusbar("코드 분석 · 68%", "사용 가능한 답 2/4", "API 41 · 파일 164");
}

function drawAtlas() {
  topbar();
  navRail(0);
  text(250, 96, "전체 백엔드 맵", 20, 700, colors.ink);
  text(250, 119, "도메인별로 묶인 API·코드·DB를 훑고 대상을 선택하세요.", 11, 400, colors.ink3);
  pill(1028, 82, "도메인", { width: 74, height: 30, fill: colors.blueSoft, color: colors.blue });
  pill(1108, 82, "계층", { width: 62, height: 30, fill: colors.raised, stroke: colors.line, color: colors.ink3 });
  pill(1176, 82, "목록", { width: 62, height: 30, fill: colors.raised, stroke: colors.line, color: colors.ink3 });
  button(1250, 78, 92, "필터", false);
  rect(244, 138, 1134, 690, "#f6f9fd", colors.line, 6, 1);
  for (let gx = 268; gx < 1360; gx += 24) {
    for (let gy = 162; gy < 812; gy += 24) circle(gx, gy, 1, "#dbe5f1");
  }
  path("M480 330 C550 330 560 330 628 330", colors.blue, 2, "", "arrowBlue");
  path("M868 330 C930 330 946 330 1012 330", colors.orange, 1.5, "6 5", "arrowOrange");
  path("M628 550 C578 620 530 622 480 650", colors.blue, 2, "", "arrowBlue");
  path("M868 550 C922 614 958 620 1012 650", colors.blue, 2, "", "arrowBlue");
  domainCard(276, 194, 306, "인증/세션", 12, ["/login", "/sessions"], ["auth", "token"], ["auth_sessions", "users"]);
  domainCard(628, 194, 306, "고객/연락처", 14, ["/users", "/contacts"], ["user", "profile"], ["users", "contacts"]);
  domainCard(980, 194, 306, "관리자", 9, ["/admin/*"], ["admin", "audit"], ["roles", "permissions"]);
  domainCard(276, 554, 306, "파일/업로드", 8, ["/files", "/uploads"], ["file", "storage"], ["files", "objects"]);
  domainCard(628, 554, 306, "회의/컨텍스트", 16, ["/sessions", "/notes"], ["meeting", "context"], ["threads", "events"]);
  domainCard(980, 554, 306, "레거시", 11, ["/legacy/*"], ["legacy-api"], ["legacy_data"]);
  pill(260, 792, "API 41", { width: 72, height: 26, fill: colors.blueSoft, color: colors.blue });
  pill(338, 792, "코드 287", { width: 82, height: 26, fill: colors.violetSoft, color: colors.violet });
  pill(426, 792, "테이블 20", { width: 88, height: 26, fill: colors.greenSoft, color: colors.green });
  evidenceTab();
  statusbar();
}

function drawSelection() {
  topbar();
  navRail(0);
  text(250, 96, "전체 백엔드 맵", 20, 700, colors.ink);
  text(250, 119, "선택 항목과 직접 연결된 대상만 또렷하게 표시됩니다.", 11, 400, colors.ink3);
  rect(244, 138, 852, 690, "#f6f9fd", colors.line, 6, 1);
  for (let gx = 268; gx < 1080; gx += 24) {
    for (let gy = 162; gy < 812; gy += 24) circle(gx, gy, 1, "#dbe5f1");
  }
  smallCard(300, 210, 260, 86, "GET /api/v1/sessions", ["Route · server/api/routes.ts:42"], "blue");
  smallCard(672, 210, 260, 86, "listSessions", ["Handler · session_handler.ts:18"], "green");
  smallCard(486, 402, 260, 86, "sessionService.list", ["Function · services/session.ts:31"], "violet");
  smallCard(300, 610, 260, 86, "findActiveSessions", ["Repository · repositories/session.ts:55"], "green");
  smallCard(672, 610, 260, 86, "auth_sessions", ["DB 후보 · 검증 필요"], "orange");
  path("M560 253 C610 253 622 253 672 253", colors.blue, 2, "", "arrowBlue");
  path("M802 296 C802 354 748 386 696 414", colors.blue, 2, "", "arrowBlue");
  path("M486 445 C420 486 416 548 430 610", colors.blue, 2, "", "arrowBlue");
  path("M746 445 C820 498 836 548 802 610", colors.orange, 1.5, "6 5", "arrowOrange");
  rect(1112, 62, 328, 814, colors.raised);
  line(1112, 62, 1112, 876, colors.line2, 1);
  text(1136, 96, "선택한 대상", 11, 700, colors.ink3);
  pill(1136, 114, "GET", { width: 54, height: 28, fill: colors.blueSoft, color: colors.blue, mono: true });
  text(1136, 164, "/api/v1/sessions", 17, 700, colors.ink, "start", true);
  text(1136, 188, "Route", 11, 500, colors.ink3);
  divider(1136, 208, 280);
  text(1136, 238, "직접 연결", 12, 700, colors.ink2);
  text(1398, 238, "3", 12, 700, colors.green, "end");
  multiline(1136, 270, ["HANDLES · listSessions", "CALLS · sessionService.list", "파일 · server/api/routes.ts:42"], 11, 500, colors.ink2, 25, true);
  text(1136, 372, "DB 후보", 12, 700, colors.ink2);
  text(1398, 372, "1", 12, 700, colors.orange, "end");
  smallCard(1136, 392, 264, 74, "auth_sessions", ["이름/경로 후보 · 검증 필요"], "orange");
  divider(1136, 494, 280);
  text(1136, 524, "다음 행동", 12, 700, colors.ink2);
  button(1136, 544, 264, "API 경로 보기", true);
  button(1136, 594, 264, "근거 3개 열기", false);
  text(1136, 658, "클릭은 선택만 바꾸고", 10, 400, colors.ink3);
  text(1136, 677, "화면 이동은 명시적 버튼으로만 합니다.", 10, 400, colors.ink3);
  statusbar("선택 유지 · GET /api/v1/sessions", "Esc 선택 해제 · Enter 답 열기", "직접 3 · 후보 1");
}

function drawSearch() {
  topbar();
  navRail(0);
  rect(224, 62, 1216, 814, "#d7e0ec", "none", 0, 1, 0.72);
  domainCard(284, 186, 300, "인증/세션", 12, ["/login", "/sessions"], ["auth", "token"], ["sessions", "users"], false, true);
  domainCard(634, 186, 300, "고객/연락처", 14, ["/users"], ["profile"], ["contacts"], false, true);
  rect(338, 124, 770, 640, colors.raised, colors.line2, 8, 1);
  text(366, 160, "프로젝트 전체 찾기", 14, 700, colors.ink);
  pill(1028, 140, "Esc", { width: 50, height: 24, fill: colors.inset, color: colors.ink3 });
  rect(366, 182, 714, 48, colors.panel, colors.blue, 6, 2);
  circle(390, 204, 7, "none", colors.blue, 1.5);
  line(395, 210, 402, 217, colors.blue, 1.5);
  text(414, 212, "/sessions", 15, 550, colors.ink, "start", true);
  text(1062, 212, "Ctrl K", 10, 600, colors.ink3, "end");
  text(366, 260, "API 라우트", 11, 700, colors.ink3);
  text(1078, 260, "3", 11, 700, colors.ink3, "end");
  resultRow(366, 274, 714, "GET", "/api/v1/sessions", "server/api/routes.ts:42 · 확정 라우트", "blue", true);
  resultRow(366, 334, 714, "POST", "/api/v1/sessions", "server/api/routes.ts:88 · 확정 라우트", "blue");
  text(366, 414, "코드", 11, 700, colors.ink3);
  resultRow(366, 428, 714, "FUNC", "listSessions", "server/handlers/session_handler.ts:18", "violet");
  resultRow(366, 488, 714, "FILE", "session_service.ts", "server/services/session_service.ts", "violet");
  text(366, 568, "데이터베이스", 11, 700, colors.ink3);
  resultRow(366, 582, 714, "TABLE", "auth_sessions", "public · 컬럼 6 · FK 1", "green");
  divider(366, 652, 714);
  pill(366, 674, "↑↓ 이동", { width: 76, height: 26, fill: colors.inset, color: colors.ink3 });
  pill(450, 674, "Enter 선택", { width: 96, height: 26, fill: colors.inset, color: colors.ink3 });
  pill(554, 674, "Esc 닫기", { width: 82, height: 26, fill: colors.inset, color: colors.ink3 });
  text(1080, 692, "선택하면 원래 화면으로 돌아가 대상만 포커스", 10, 500, colors.ink3, "end");
  statusbar("검색 · /sessions", "검색은 새 페이지를 만들지 않음", "결과 6");
}

function lane(x, y, w, number, titleValue, subtitle, tone = "green") {
  const color = tone === "orange" ? colors.orange : tone === "blue" ? colors.blue : tone === "red" ? colors.red : colors.green;
  const soft = tone === "orange" ? colors.orangeSoft : tone === "blue" ? colors.blueSoft : tone === "red" ? colors.redSoft : colors.greenSoft;
  rect(x, y, w, 438, colors.raised, colors.line2, 6, 1);
  rect(x, y, w, 70, soft, "none", 6);
  rect(x, y, w, 4, color);
  text(x + 14, y + 30, number, 15, 750, color, "start", true);
  text(x + 56, y + 30, titleValue, 14, 700, colors.ink);
  text(x + 56, y + 50, subtitle, 10, 400, colors.ink3);
}

function laneItem(x, y, w, titleValue, meta, tone = "green") {
  const color = tone === "orange" ? colors.orange : tone === "red" ? colors.red : tone === "blue" ? colors.blue : colors.green;
  rect(x, y, w, 78, colors.raised, colors.line, 5, 1);
  rect(x, y, 4, 78, color, "none", 4);
  text(x + 14, y + 24, titleValue, 12, 650, colors.ink, "start", true);
  text(x + 14, y + 46, meta, 10, 400, colors.ink3);
  text(x + 14, y + 64, tone === "orange" ? "후보 · 검증 필요" : tone === "red" ? "미확인" : "확정 근거", 9, 650, color);
}

function drawApiPath() {
  topbar();
  navRail(1);
  text(250, 92, "전체 구조  /  API가 닿는 코드", 11, 600, colors.ink3);
  pill(250, 112, "GET", { width: 54, height: 28, fill: colors.blueSoft, color: colors.blue, mono: true });
  text(318, 134, "/api/v1/sessions", 20, 700, colors.ink, "start", true);
  pill(1168, 108, "부분 답변", { width: 96, height: 30, fill: colors.orangeSoft, color: colors.orange });
  button(1274, 104, 96, "근거 5", false);
  const lx = [244, 468, 692, 916, 1140];
  const labels = [
    ["01", "Route", "선택한 API 진입점", "blue"],
    ["02", "Handler", "확정 HANDLES", "green"],
    ["03", "Service / Function", "확정 CALLS", "green"],
    ["04", "Repository / Query", "데이터 접근 코드", "green"],
    ["05", "DB 후보", "경로 뒤 검증 후보", "orange"],
  ];
  labels.forEach(([num, label, sub, tone], index) => lane(lx[index], 166, 208, num, label, sub, tone));
  laneItem(256, 254, 184, "/api/v1/sessions", "routes.ts:42", "blue");
  laneItem(480, 254, 184, "listSessions", "session_handler.ts:18", "green");
  laneItem(704, 254, 184, "sessionService.list", "services/session.ts:31", "green");
  laneItem(928, 254, 184, "findActive", "repositories/session.ts:55", "green");
  laneItem(1152, 254, 184, "auth_sessions", "이름/쿼리 후보", "orange");
  laneItem(704, 350, 184, "applyVisibility", "services/session.ts:49", "green");
  path("M440 293 L480 293", colors.blue, 2, "", "arrowBlue");
  path("M664 293 L704 293", colors.blue, 2, "", "arrowBlue");
  path("M888 293 L928 293", colors.blue, 2, "", "arrowBlue");
  path("M1112 293 L1152 293", colors.orange, 1.5, "6 5", "arrowOrange");
  smallCard(244, 628, 542, 126, "확인 안 된 구간 · 2", ["Repository 이후 실제 SQL 바인딩은 찾지 못했습니다.", "DB 미사용이 확정된 것은 아닙니다."], "red");
  smallCard(802, 628, 568, 126, "권장 확인 · 2", ["auth_sessions 후보의 쿼리 근거를 확인하세요.", "스냅샷 범위가 최신인지 다시 확인하세요."], "blue");
  evidenceTab();
  statusbar("API 읽기 · /api/v1/sessions", "확정 경로와 DB 후보 분리", "확정 5 · 후보 1 · 미확인 2");
}

function usageCard(x, y, w, titleValue, pathValue, kind, tone = "green") {
  rect(x, y, w, 86, colors.raised, colors.line, 6, 1);
  pill(x + 14, y + 13, kind, { width: 66, height: 26, fill: tone === "orange" ? colors.orangeSoft : colors.greenSoft, color: tone === "orange" ? colors.orange : colors.green, mono: true });
  text(x + 94, y + 30, titleValue, 12, 650, colors.ink, "start", true);
  text(x + 14, y + 60, pathValue, 10, 400, colors.ink3, "start", true);
  text(x + w - 14, y + 60, tone === "orange" ? "후보" : "확정", 10, 700, tone === "orange" ? colors.orange : colors.green, "end");
}

function drawTableUsage() {
  topbar();
  navRail(2);
  text(250, 92, "전체 구조  /  테이블 사용처", 11, 600, colors.ink3);
  text(250, 126, "public.auth_sessions", 20, 700, colors.ink, "start", true);
  pill(476, 104, "TABLE", { width: 70, height: 28, fill: colors.greenSoft, color: colors.green, mono: true });
  text(250, 151, "이 테이블을 읽고 쓰는 코드와 DB 구조를 분리해서 보여줍니다.", 11, 400, colors.ink3);
  rect(244, 176, 442, 550, colors.panel, colors.line2, 6, 1);
  text(264, 210, "읽는 코드", 14, 700, colors.ink);
  pill(610, 190, "확정 2", { width: 64, height: 26, fill: colors.greenSoft, color: colors.green });
  usageCard(264, 232, 402, "findActiveSessions", "repositories/session.ts:55", "QUERY", "green");
  usageCard(264, 328, 402, "getSessionByToken", "repositories/token.ts:27", "QUERY", "green");
  usageCard(264, 424, 402, "sessionCleanup", "jobs/session_cleanup.ts:19", "BATCH", "orange");
  smallCard(264, 548, 402, 128, "읽기 요약", ["확정 코드 2개", "후보 배치 1개", "표시 범위는 현재 스냅샷 기준"], "green");
  rect(702, 176, 442, 550, colors.panel, colors.line2, 6, 1);
  text(722, 210, "쓰는 코드", 14, 700, colors.ink);
  pill(1066, 190, "확정 1", { width: 64, height: 26, fill: colors.greenSoft, color: colors.green });
  usageCard(722, 232, 402, "createSession", "repositories/session.ts:91", "INSERT", "green");
  usageCard(722, 328, 402, "touchLastSeen", "repositories/session.ts:118", "UPDATE", "orange");
  smallCard(722, 452, 402, 128, "쓰기 요약", ["확정 코드 1개", "후보 업데이트 1개", "삭제 경로는 찾지 못함"], "orange");
  rect(1160, 176, 210, 550, colors.raised, colors.line2, 6, 1);
  text(1178, 210, "DB 구조", 14, 700, colors.ink);
  text(1178, 248, "PK", 10, 700, colors.ink3);
  text(1212, 248, "id", 11, 600, colors.ink, "start", true);
  text(1178, 278, "FK", 10, 700, colors.ink3);
  text(1212, 278, "user_id → users.id", 10, 600, colors.ink, "start", true);
  text(1178, 308, "컬럼", 10, 700, colors.ink3);
  multiline(1178, 330, ["id", "user_id", "client_type", "created_at", "expires_at", "last_seen_at"], 10, 500, colors.ink2, 24, true);
  divider(1178, 490, 174);
  text(1178, 522, "연결 테이블", 11, 700, colors.ink2);
  pill(1178, 540, "users", { width: 68, height: 27, fill: colors.greenSoft, color: colors.green, mono: true });
  button(1178, 590, 174, "컬럼 영향 보기", false);
  evidenceTab();
  statusbar("테이블 · public.auth_sessions", "읽기/쓰기 방향 고정", "확정 3 · 후보 2");
}

function impactColumn(x, y, w, titleValue, count, tone, items) {
  const color = tone === "red" ? colors.red : tone === "orange" ? colors.orange : tone === "blue" ? colors.blue : colors.green;
  const soft = tone === "red" ? colors.redSoft : tone === "orange" ? colors.orangeSoft : tone === "blue" ? colors.blueSoft : colors.greenSoft;
  rect(x, y, w, 500, colors.panel, colors.line2, 6, 1);
  rect(x, y, w, 60, soft, "none", 6);
  rect(x, y, w, 4, color);
  text(x + 16, y + 28, titleValue, 13, 700, colors.ink);
  pill(x + w - 48, y + 15, String(count), { width: 32, height: 28, fill: colors.raised, color, stroke: colors.line });
  items.forEach((item, index) => {
    const iy = y + 78 + index * 112;
    rect(x + 12, iy, w - 24, 96, colors.raised, colors.line, 5, 1);
    text(x + 26, iy + 26, item[0], 11, 650, colors.ink, "start", item[2] ?? false);
    text(x + 26, iy + 49, item[1], 9, 400, colors.ink3, "start", true);
    pill(x + 26, iy + 62, tone === "green" ? "확정" : tone === "red" ? "알 수 없음" : tone === "orange" ? "후보" : "행동", { width: tone === "red" ? 78 : 58, height: 22, size: 9, fill: soft, color });
  });
}

function drawImpact() {
  topbar();
  navRail(3);
  text(250, 92, "전체 구조  /  변경 영향", 11, 600, colors.ink3);
  text(250, 126, "auth_sessions.user_id", 20, 700, colors.ink, "start", true);
  pill(514, 104, "FK", { width: 42, height: 28, fill: colors.greenSoft, color: colors.green, mono: true });
  pill(564, 104, "uuid", { width: 58, height: 28, fill: colors.inset, color: colors.ink2, mono: true });
  pill(630, 104, "NOT NULL", { width: 86, height: 28, fill: colors.inset, color: colors.ink2, mono: true });
  pill(1198, 104, "부분 답변", { width: 96, height: 30, fill: colors.orangeSoft, color: colors.orange });
  button(1302, 100, 68, "근거", false);
  impactColumn(244, 164, 270, "직접 영향", 3, "green", [
    ["users.id FK", "schema/public/auth_sessions"],
    ["createSession", "repositories/session.ts:91"],
    ["findActiveSessions", "repositories/session.ts:55"],
  ]);
  impactColumn(526, 164, 270, "코드 영향 후보", 2, "orange", [
    ["sessionCleanup", "jobs/session_cleanup.ts:19"],
    ["touchLastSeen", "repositories/session.ts:118"],
  ]);
  impactColumn(808, 164, 270, "확인되지 않은 범위", 2, "red", [
    ["마이그레이션", "엔진 근거 없음"],
    ["외부 배치", "스냅샷 외부 가능성"],
  ]);
  impactColumn(1090, 164, 280, "변경 전 확인", 3, "blue", [
    ["FK 대상과 타입 확인", "users.id · uuid"],
    ["NULL 정책 검토", "현재 NOT NULL"],
    ["최신 스냅샷 재확인", "분석 시점 · 방금 전"],
  ]);
  rect(244, 682, 1126, 64, colors.raised, colors.line2, 6, 1);
  text(264, 710, "결론", 11, 700, colors.ink3);
  text(324, 710, "직접 영향 3개는 근거가 있고, 코드 후보 2개와 스냅샷 밖 범위는 별도 확인이 필요합니다.", 12, 600, colors.ink);
  button(1190, 695, 160, "검토 체크 시작", true);
  evidenceTab();
  statusbar("컬럼 · auth_sessions.user_id", "직접 → 후보 → 미확인 → 행동", "확정 3 · 후보 2 · 미확인 2");
}

function codeLine(y, number, code, highlight = false, accent = false) {
  if (highlight) rect(268, y - 17, 724, 24, accent ? colors.orangeSoft : colors.blueSoft);
  text(288, y, String(number), 10, 500, colors.ink4, "end", true);
  text(310, y, code, 11, 500, highlight && accent ? colors.orange : colors.ink2, "start", true);
}

function drawEvidence() {
  topbar();
  navRail(1);
  text(250, 92, "API 경로  /  선택 근거", 11, 600, colors.ink3);
  text(250, 126, "listSessions → sessionService.list", 18, 700, colors.ink, "start", true);
  button(866, 102, 126, "이전 근거", false);
  rect(244, 158, 770, 590, colors.raised, colors.line2, 6, 1);
  rect(244, 158, 770, 46, colors.panel, "none", 6);
  text(264, 187, "server/handlers/session_handler.ts", 11, 600, colors.ink2, "start", true);
  pill(906, 168, "L18–27", { width: 82, height: 26, fill: colors.inset, color: colors.ink3, mono: true });
  codeLine(238, 15, "export async function listSessions(req, res) {");
  codeLine(270, 16, "  const userId = req.user.id;");
  codeLine(302, 17, "  const filters = parseSessionFilters(req.query);");
  codeLine(334, 18, "  const sessions = await sessionService.list(", true);
  codeLine(366, 19, "    userId, filters", true);
  codeLine(398, 20, "  );", true);
  codeLine(430, 21, "  return res.json({ sessions });");
  codeLine(462, 22, "}");
  divider(264, 494, 724);
  text(264, 524, "왜 이 근거인가", 12, 700, colors.ink);
  multiline(264, 550, ["CALLS 관계가 함수 ID와 파일 위치를 함께 가리킵니다.", "이 줄을 선택하면 경로의 해당 연결만 강조됩니다."], 11, 400, colors.ink3, 20);
  button(264, 608, 122, "파일 열기", true);
  button(398, 608, 112, "경로 복사", false);
  rect(1030, 62, 410, 814, colors.raised);
  line(1030, 62, 1030, 876, colors.line2, 1);
  text(1054, 96, "근거 상세", 15, 700, colors.ink);
  pill(1054, 118, "확정", { width: 58, height: 27, fill: colors.greenSoft, color: colors.green });
  text(1124, 138, "CALLS", 11, 650, colors.ink2, "start", true);
  divider(1054, 162, 362);
  text(1054, 194, "출발", 10, 700, colors.ink3);
  multiline(1054, 218, ["listSessions", "session_handler.ts:18"], 11, 600, colors.ink, 20, true);
  text(1054, 278, "도착", 10, 700, colors.ink3);
  multiline(1054, 302, ["sessionService.list", "services/session.ts:31"], 11, 600, colors.ink, 20, true);
  divider(1054, 350, 362);
  text(1054, 382, "출처", 10, 700, colors.ink3);
  text(1054, 406, "code engine inventory", 11, 600, colors.ink2, "start", true);
  text(1054, 450, "분석 기준 시점", 10, 700, colors.ink3);
  text(1054, 474, "2026-07-12 14:32 KST", 11, 600, colors.ink2, "start", true);
  text(1054, 518, "판정 규칙", 10, 700, colors.ink3);
  multiline(1054, 542, ["명시적 CALLS 관계 + 양쪽 파일 위치", "후보 추론이 섞이지 않은 구조 근거"], 11, 400, colors.ink3, 20);
  smallCard(1054, 608, 362, 104, "선택 동작", ["근거 클릭 → 소스 줄 강조", "Esc → 서랍 닫기, 경로 선택은 유지"], "blue");
  statusbar("근거 · CALLS", "파일/라인/판정 규칙까지 추적", "근거 1/5");
}

function stateCard(x, y, w, titleValue, badge, lines, action, tone) {
  const color = tone === "red" ? colors.red : tone === "orange" ? colors.orange : tone === "blue" ? colors.blue : colors.green;
  const soft = tone === "red" ? colors.redSoft : tone === "orange" ? colors.orangeSoft : tone === "blue" ? colors.blueSoft : colors.greenSoft;
  rect(x, y, w, 230, colors.raised, colors.line2, 6, 1);
  rect(x, y, w, 5, color);
  pill(x + 18, y + 20, badge, { height: 26, fill: soft, color });
  text(x + 18, y + 78, titleValue, 15, 700, colors.ink);
  multiline(x + 18, y + 106, lines, 11, 400, colors.ink3, 19);
  button(x + 18, y + 172, 150, action, tone === "blue");
}

function drawStates() {
  topbar({ project: "프로젝트 상태", code: "상태별", db: "복구 UX", search: false });
  titleBlock(72, 112, "가짜 데이터 없이", "빈·부분·오래된 결과를 복구하는 방식", ["문제 상태에서도 현재 사용할 수 있는 답과 다음 한 행동을 분명하게 남깁니다."]);
  stateCard(72, 194, 404, "프로젝트가 없음", "EMPTY", ["분석 결과 대신 연결 단계만 표시", "최근 프로젝트가 있으면 목록으로 제공"], "프로젝트 열기", "blue");
  stateCard(492, 194, 404, "분석이 일부 완료됨", "PARTIAL", ["준비된 답은 즉시 개방", "대기 중인 답에는 필요한 소스를 표시"], "전체 구조 보기", "green");
  stateCard(912, 194, 404, "스냅샷이 오래됨", "STALE", ["기존 결과는 흐리게 유지", "변경된 소스와 분석 기준 시점을 함께 표시"], "다시 분석", "orange");
  stateCard(72, 444, 404, "관계를 찾지 못함", "NO LINKS", ["'없음'으로 단정하지 않음", "읽은 항목과 미확인 범위를 분리"], "목록으로 보기", "orange");
  stateCard(492, 444, 404, "DB 연결 실패", "DB ERROR", ["코드 기반 답은 그대로 유지", "DB가 필요한 보기만 잠그고 이유를 표시"], "연결 확인", "red");
  stateCard(912, 444, 404, "엔진 오류", "ENGINE", ["오류 메시지를 사용자 언어로 번역", "진단 복사는 고급 메뉴에 배치"], "다시 시도", "red");
  rect(72, 710, 1244, 70, colors.panel, colors.line2, 6, 1);
  text(94, 739, "0번 규칙", 11, 700, colors.red);
  text(174, 739, "실데이터로 채울 수 없는 슬롯과 가짜 fallback 숫자는 렌더하지 않습니다.", 13, 650, colors.ink);
  text(94, 762, "복구 중에도 마지막으로 검증된 결과와 선택 위치는 유지합니다.", 10, 400, colors.ink3);
  statusbar("상태 UX", "한 상태 · 한 설명 · 한 행동", "가짜 데이터 0");
}

function specPanel(x, y, titleValue, subtitle, render) {
  out.push(`<g transform="translate(${x} ${y})">`);
  rect(0, 0, SCREEN_W, SCREEN_H, colors.raised, colors.line2, 8, 1);
  rect(0, 0, SCREEN_W, 86, colors.panel, "none", 8);
  text(30, 38, titleValue, 19, 700, colors.ink);
  text(30, 65, subtitle, 12, 400, colors.ink3);
  line(0, 85, SCREEN_W, 85, colors.line, 1);
  render();
  out.push("</g>");
}

function specRow(y, gesture, result, invariant, key = "") {
  rect(36, y, 1368, 70, y % 140 === 0 ? colors.panel : colors.raised, colors.line, 4, 1);
  text(58, y + 28, gesture, 12, 700, colors.ink);
  text(58, y + 49, key, 10, 500, colors.blue, "start", true);
  text(330, y + 28, result, 12, 600, colors.ink2);
  text(330, y + 50, invariant, 10, 400, colors.ink3);
}

function drawInteractionSpec() {
  text(36, 122, "입력", 11, 700, colors.ink3);
  text(330, 122, "결과와 고정 규칙", 11, 700, colors.ink3);
  const rows = [
    ["항목 1회 클릭", "선택 + 1-hop 강조 + 인스펙터 열기", "화면 이동과 줌 위치는 바뀌지 않음", "Click"],
    ["답 열기", "현재 대상의 API/테이블/영향 화면으로 이동", "Back 시 선택·필터·줌을 그대로 복원", "Enter / CTA"],
    ["통합 검색", "현재 화면 위에 검색 팔레트 표시", "결과 선택 후 원래 화면으로 돌아가 포커스", "Ctrl K"],
    ["근거 열기", "오른쪽 서랍에 파일·라인·판정 규칙 표시", "중앙 결론은 가리지 않고 폭만 줄임", "E"],
    ["닫기/취소", "최상단 오버레이 → 서랍 → 선택 순서로 닫기", "한 번의 Esc가 두 단계 이상을 없애지 않음", "Esc"],
    ["새로 분석", "기존 결과 위에 진행 상태 표시", "새 분석 성공 전까지 마지막 검증 결과 유지", "Refresh"],
    ["목록 축소", "상위 항목 +N 접기로 밀도 조절", "사용자가 고른 항목은 접힘 대상에서 제외", "+N"],
    ["잠긴 보기", "필요한 소스와 한 가지 해제 행동 표시", "죽은 버튼 대신 이유를 읽을 수 있음", "Locked"],
  ];
  rows.forEach((row, index) => specRow(142 + index * 82, ...row));
  rect(36, 824, 1368, 98, colors.blueSoft, "#a9c5ff", 6, 1);
  text(58, 856, "최우선 검증", 11, 700, colors.blue);
  text(58, 882, "처음 온 사용자가 5초 안에 클릭할 곳을 알고, 세 번 이내의 행동으로 답과 근거에 도달해야 합니다.", 13, 650, colors.ink);
}

function trustRow(y, label, meaning, presentation, action, tone) {
  const color = tone === "red" ? colors.red : tone === "orange" ? colors.orange : tone === "blue" ? colors.blue : colors.green;
  const soft = tone === "red" ? colors.redSoft : tone === "orange" ? colors.orangeSoft : tone === "blue" ? colors.blueSoft : colors.greenSoft;
  pill(42, y, label, { width: 112, height: 32, fill: soft, color });
  text(184, y + 21, meaning, 12, 600, colors.ink2);
  text(568, y + 21, presentation, 11, 500, colors.ink3);
  text(1040, y + 21, action, 11, 600, color);
  divider(42, y + 48, 1350);
}

function drawTrustSpec() {
  text(42, 126, "상태", 11, 700, colors.ink3);
  text(184, 126, "의미", 11, 700, colors.ink3);
  text(568, 126, "화면 표현", 11, 700, colors.ink3);
  text(1040, 126, "사용자 행동", 11, 700, colors.ink3);
  trustRow(150, "확정", "엔진 구조 관계와 위치 근거가 함께 있음", "실선 · 녹색 상태 · 파일/라인 제공", "바로 검토 가능", "green");
  trustRow(224, "후보", "이름·경로·쿼리 등 추가 확인이 필요함", "점선 · 주황 상태 · 후보 이유 제공", "근거 열어 확인", "orange");
  trustRow(298, "미확인", "현재 스냅샷과 엔진 범위에서 알 수 없음", "빈 칸 대신 명시적 미확인 카드", "범위 확대/재분석", "red");
  trustRow(372, "오래됨", "소스 지문이 저장 시점과 달라짐", "결과 위 stale 리본 · 기준 시점 표시", "다시 분석", "orange");
  trustRow(446, "부분", "일부 소스만 준비됐거나 표시 한도에 닿음", "+N 접힘과 준비된 답만 활성화", "필요할 때 더 보기", "blue");
  rect(42, 548, 1350, 150, colors.redSoft, "#f4a8a8", 6, 1);
  text(66, 582, "절대 금지", 12, 750, colors.red);
  multiline(66, 612, ["• 워크스페이스명, 경로, 개수, 신뢰도 퍼센트의 가짜 fallback", "• 엔진이 감지하지 않는 Middleware/Test/Migration 슬롯을 임의 데이터로 채우기", "• 후보 관계를 확정 관계처럼 같은 색과 선으로 표시하기"], 12, 550, colors.ink2, 27);
  rect(42, 722, 1350, 168, colors.greenSoft, "#9bd9c5", 6, 1);
  text(66, 758, "일관된 읽기 문법", 12, 750, colors.green);
  multiline(66, 790, ["1. 위에서 현재 프로젝트와 데이터 신선도를 확인", "2. 왼쪽에서 찾을 답을 고정된 네 위치에서 선택", "3. 중앙에서 결론과 범위를 읽고, 오른쪽에서 근거를 검증", "4. 뒤로 가면 이전 선택과 맥락이 그대로 복원"], 12, 550, colors.ink2, 26);
}

out.push(`<?xml version="1.0" encoding="UTF-8"?>`);
out.push(`<svg xmlns="http://www.w3.org/2000/svg" width="${WIDTH}" height="${HEIGHT}" viewBox="0 0 ${WIDTH} ${HEIGHT}">`);
out.push(`<defs>
  <filter id="shadow" x="-20%" y="-20%" width="140%" height="140%"><feDropShadow dx="0" dy="8" stdDeviation="14" flood-color="#0f172a" flood-opacity="0.10"/></filter>
  <marker id="arrowBlue" markerWidth="7" markerHeight="7" refX="6" refY="3.5" orient="auto"><path d="M0,0 L7,3.5 L0,7 z" fill="#2563eb"/></marker>
  <marker id="arrowOrange" markerWidth="7" markerHeight="7" refX="6" refY="3.5" orient="auto"><path d="M0,0 L7,3.5 L0,7 z" fill="#d97706"/></marker>
  <style>
    .sans { font-family: Inter, "Noto Sans KR", "Segoe UI", sans-serif; letter-spacing: 0; }
    .mono { font-family: "JetBrains Mono", "Cascadia Code", Consolas, monospace; letter-spacing: 0; }
  </style>
</defs>`);
rect(0, 0, WIDTH, HEIGHT, colors.app);

text(80, 78, "BACKEND VISUAL MAP · UX CONCEPT 01", 13, 750, colors.blue);
text(80, 126, "Atlas Explorer", 44, 750, colors.ink);
multiline(80, 164, ["전체 백엔드 지도를 먼저 보고, 선택한 대상을 같은 자리에서 흐름·사용처·영향·근거로 좁혀 가는 UX.", "아래 예시 항목과 수치는 레이아웃 검증용이며 실제 제품에서는 분석 결과만 사용합니다."], 15, 400, colors.ink3, 24);

rect(80, 230, 4420, 124, colors.raised, colors.line2, 8, 1);
text(108, 264, "PRIMARY FLOW", 10, 750, colors.ink3);
const flow = ["S01 연결", "S02 분석", "S03 전체 맵", "S04 선택", "S06·07·08 답", "S09 근거"];
let flowX = 108;
flow.forEach((label, index) => {
  const w = pill(flowX, 284, label, { width: index === 4 ? 132 : 104, height: 34, fill: index === 0 ? colors.blueSoft : colors.panel, stroke: index === 0 ? "#a9c5ff" : colors.line2, color: index === 0 ? colors.blue : colors.ink2 });
  flowX += w + 34;
  if (index < flow.length - 1) path(`M${flowX - 26} 301 L${flowX - 8} 301`, colors.blue, 1.5, "", "arrowBlue");
});
text(952, 306, "Ctrl K → S05 검색", 12, 650, colors.blue);
text(1120, 306, "문제 상태 → S10 복구", 12, 650, colors.orange);
text(4470, 304, "공통 위치: 상단 프로젝트/검색/상태 · 왼쪽 4개 답 · 중앙 결론 · 오른쪽 근거", 11, 550, colors.ink3, "end");

const xs = [80, 1570, 3060];
const ys = [400, 1410, 2420, 3430];
screen(xs[0], ys[0], "S01 · 프로젝트 연결", "코드 먼저 · DB는 선택", drawSetup);
screen(xs[1], ys[0], "S02 · 분석 진행", "완료를 기다리지 않는 UX", drawIndexing);
screen(xs[2], ys[0], "S03 · 전체 백엔드 맵", "도메인별 스캔과 선택", drawAtlas);
screen(xs[0], ys[1], "S04 · 항목 선택 상태", "클릭은 선택 · 이동은 명시적", drawSelection);
screen(xs[1], ys[1], "S05 · 통합 검색", "현재 맥락 위에 여는 팔레트", drawSearch);
screen(xs[2], ys[1], "S06 · API가 닿는 코드", "경로와 미확인 범위", drawApiPath);
screen(xs[0], ys[2], "S07 · 테이블 사용처", "읽기/쓰기와 DB 구조", drawTableUsage);
screen(xs[1], ys[2], "S08 · 컬럼 변경 영향", "직접·후보·미확인·행동", drawImpact);
screen(xs[2], ys[2], "S09 · 근거/소스 상세", "결론을 검증하는 오른쪽 서랍", drawEvidence);
screen(xs[0], ys[3], "S10 · 복구 상태", "빈·부분·오래됨·오류", drawStates);
specPanel(xs[1], ys[3], "UX Interaction Contract", "모든 화면에서 바뀌지 않는 조작 규칙", drawInteractionSpec);
specPanel(xs[2], ys[3], "Trust & Data Contract", "신뢰도를 퍼센트가 아니라 근거 상태로 표현", drawTrustSpec);

out.push("</svg>");

const outputPath = join(dirname(fileURLToPath(import.meta.url)), "figma-concept-01-atlas-explorer.svg");
writeFileSync(outputPath, out.join("\n"), "utf8");
console.log(outputPath);
