// Role: Exercise the standalone viewer's DOM events without external packages.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

// Only browser primitives used by the viewer are represented here. Events bubble
// through the fixture tree, so tests exercise the production delegated handlers.
class Element {
  constructor(name, attributes = {}) {
    this.localName = name;
    this.attributes = { ...attributes };
    this.children = [];
    this.parentElement = null;
    this.listeners = new Map();
    this.style = {};
    this.hidden = false;
    this._text = "";
    this.value = "";
    this.clientWidth = 1200;
    this.clientHeight = 800;
    this.captured = new Set();
  }
  get dataset() {
    return Object.fromEntries(Object.entries(this.attributes).filter(([key]) => key.startsWith("data-")).map(([key, value]) => [key.slice(5).replace(/-([a-z])/g, (_, letter) => letter.toUpperCase()), value]));
  }
  get className() { return this.attributes.class || ""; }
  set className(value) { this.attributes.class = value; }
  get classList() {
    return {
      contains: (value) => this.className.split(/\s+/).includes(value),
      toggle: (value, force) => {
        const classes = new Set(this.className.split(/\s+/).filter(Boolean));
        const enabled = force ?? !classes.has(value);
        if (enabled) classes.add(value); else classes.delete(value);
        this.className = [...classes].join(" ");
        return enabled;
      },
      add: (value) => this.classList.toggle(value, true),
      remove: (value) => this.classList.toggle(value, false),
    };
  }
  get textContent() { return this._text + this.children.map((child) => child.textContent).join(""); }
  set textContent(value) { this._text = String(value); this.replaceChildren(); }
  get firstElementChild() { return this.children[0] || null; }
  get options() { return this.children.filter((child) => child.localName === "option"); }
  get viewBox() {
    const [x, y, width, height] = (this.attributes.viewBox || "0 0 0 0").split(" ").map(Number);
    return { baseVal: { x, y, width, height } };
  }
  get width() { return { baseVal: { value: Number(this.attributes.width || 0) } }; }
  get height() { return { baseVal: { value: Number(this.attributes.height || 0) } }; }
  setAttribute(name, value) { this.attributes[name] = String(value); }
  append(...items) { for (const item of items) { item.parentElement = this; this.children.push(item); } }
  replaceChildren(...items) { for (const child of this.children) child.parentElement = null; this.children = []; this.append(...items); }
  contains(element) { for (let current = element; current; current = current.parentElement) if (current === this) return true; return false; }
  matches(selector) {
    return selector.split(",").some((raw) => {
      const part = raw.trim();
      const tag = part.match(/^[a-z]+/i)?.[0];
      if (tag && tag !== this.localName) return false;
      for (const [, value] of part.matchAll(/\.([\w-]+)/g)) if (!this.classList.contains(value)) return false;
      for (const [, name, value] of part.matchAll(/\[([\w-]+)(?:="([^"]*)")?\]/g)) {
        if (!(name in this.attributes) || (value !== undefined && this.attributes[name] !== value)) return false;
      }
      return true;
    });
  }
  closest(selector) { for (let current = this; current; current = current.parentElement) if (current.matches(selector)) return current; return null; }
  querySelectorAll(selector) { return this.children.flatMap((child) => [...(child.matches(selector) ? [child] : []), ...child.querySelectorAll(selector)]); }
  addEventListener(type, handler) { if (!this.listeners.has(type)) this.listeners.set(type, []); this.listeners.get(type).push(handler); }
  fire(type, fields = {}) {
    const event = { target: this, relatedTarget: null, button: 0, pointerId: 1, clientX: 0, clientY: 0, deltaMode: 0, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, ...fields };
    for (let current = this; current; current = current.parentElement) for (const handler of current.listeners.get(type) || []) handler(event);
    return event;
  }
  setPointerCapture(id) { this.captured.add(id); }
  hasPointerCapture(id) { return this.captured.has(id); }
  releasePointerCapture(id) { this.captured.delete(id); }
  getBoundingClientRect() { return { left: 0, top: 0, width: this.clientWidth, height: this.clientHeight }; }
}

function parseSvg(source) {
  const holder = new Element("document");
  const stack = [holder];
  for (const match of source.matchAll(/<\/?[\w-]+(?:\s[^<>]*?)?\s*\/?>/g)) {
    const token = match[0];
    if (token.startsWith("</")) { stack.pop(); continue; }
    const name = token.match(/^<([\w-]+)/)[1];
    const attributes = Object.fromEntries([...token.matchAll(/([\w-]+)="([^"]*)"/g)].map(([, key, value]) => [key, value]));
    const element = new Element(name, attributes);
    stack.at(-1).append(element);
    if (!token.endsWith("/>")) stack.push(element);
  }
  return { documentElement: holder.firstElementChild };
}

const shape = (id) => `<g class="node" data-node-id="${id}"><rect/><text/></g>`;
const edge = (id, from, to) => `<g class="edge" data-edge-id="${id}" data-from="${from}" data-to="${to}"><path class="route"/><polygon class="arrowhead"/></g>`;
const svg = (content) => `<svg width="800" height="400" viewBox="0 0 800 400"><g class="graph">${content}</g></svg>`;
const crateSvg = svg(shape("app") + shape("core") + shape("support") + shape("unrelated") + edge("out", "app", "core") + edge("in", "support", "app") + `<g class="edge-label" data-edge-id="in"><text/></g><g class="bridges"/>`);
const moduleSvg = svg(
  `<g class="cluster" data-region-id="crate" data-parent=""><rect/><g class="cluster" data-region-id="crate::child" data-parent="crate"><rect/><g class="cluster" data-region-id="crate::child::leaf" data-parent="crate::child"><rect/></g></g></g>` +
  ["crate", "crate::child", "crate::child::leaf"].map((id) => shape(id) + `<g class="region-label" data-region-id="${id}"><rect/><text/></g>`).join("") +
  edge("root-leaf", "crate", "crate::child::leaf") + edge("leaf-child", "crate::child::leaf", "crate::child"),
);

function harness(overrides = {}) {
  const ids = ["architecture-data", "canvas", "world", "tree", "zoom-level", "target-select", "back", "target-control", "view-path", "empty", "empty-message", "status", "zoom-in", "zoom-out", "fit", "notices", "notice-summary", "notice-list"];
  const elements = Object.fromEntries(ids.map((id) => [id, new Element(id === "target-select" ? "select" : ["back", "zoom-in", "zoom-out", "fit"].includes(id) ? "button" : "div", { id })]));
  elements.canvas.append(elements.world);
  const input = {
    crates: crateSvg,
    modules: [
      { package: "app", target: "app", kind: "bin", test: false, svg: moduleSvg },
      { package: "app", target: "app_lib", kind: "lib", test: true, svg: moduleSvg },
      { package: "app", target: "app_lib", kind: "lib", test: false, svg: moduleSvg },
      { package: "support", target: "support", kind: "lib", test: false, svg: moduleSvg },
    ],
    notices: [],
    ...overrides,
  };
  elements["architecture-data"].textContent = JSON.stringify(input);
  const document = {
    getElementById: (id) => elements[id],
    createElement: (name) => new Element(name),
    importNode: (element) => element,
  };
  const window = new Element("window");
  const context = vm.createContext({ document, window, Element, DOMParser: class { parseFromString(text) { return parseSvg(text); } } });
  vm.runInContext(fs.readFileSync(path.join(__dirname, "viewer.js"), "utf8"), context);
  const find = (selector) => elements.world.querySelectorAll(selector)[0];
  return { ...elements, find, node: (id) => find(`.node[data-node-id="${id}"]`), region: (id) => find(`.region-label[data-region-id="${id}"]`), edge: (id) => find(`.edge[data-edge-id="${id}"]`), treeNode: (view, id) => elements.tree.querySelectorAll(`.tree-entity[data-view="${view}"][data-node-id="${id}"]`)[0] };
}

test("hover highlights direct incoming/outgoing nodes, edges, arrow owners and labels", () => {
  const page = harness();
  page.node("app").fire("pointerover");
  assert(page.node("app").classList.contains("is-selected"));
  assert(page.node("core").classList.contains("is-outgoing"));
  assert(page.node("support").classList.contains("is-incoming"));
  assert(page.node("unrelated").classList.contains("is-dim"));
  assert(page.edge("out").classList.contains("is-outgoing"));
  assert(page.edge("in").classList.contains("is-incoming"));
  assert(page.edge("out").querySelectorAll(".arrowhead")[0].closest(".is-outgoing"));
  assert(page.find(".edge-label").classList.contains("is-incoming"));
  assert(page.world.classList.contains("has-focus"));
  assert.match(page.status.textContent, /1 dependents.*1 dependencies/);
  page.node("app").fire("pointerout");
  assert(!page.world.classList.contains("has-focus"));
  assert(!page.node("unrelated").classList.contains("is-dim"));
  const css = fs.readFileSync(path.join(__dirname, "viewer.css"), "utf8");
  assert.match(css, /\.edge\.is-incoming \.arrowhead\{fill:#8b5cf6\}/);
  assert.match(css, /\.edge\.is-outgoing \.arrowhead\{fill:#0d9488\}/);
});

test("double-click prefers a normal library, target selector switches, and nested regions focus exactly", () => {
  const page = harness();
  page.node("app").fire("dblclick");
  assert.equal(page["target-select"].value, "2");
  assert.equal(page["target-select"].options.length, 3);
  assert.equal(page["target-control"].hidden, false);
  assert.match(page.status.textContent, /app_lib \/ normal/);
  page.region("crate::child::leaf").children[1].fire("pointerover");
  assert(page.region("crate::child::leaf").classList.contains("is-selected"));
  assert(!page.region("crate").classList.contains("is-selected"));
  assert(page.region("crate").classList.contains("is-incoming"));
  assert(page.region("crate::child").classList.contains("is-outgoing"));
  assert(page.edge("root-leaf").classList.contains("is-incoming"));
  assert(page.edge("leaf-child").classList.contains("is-outgoing"));
  const childCluster = page.find('.cluster[data-region-id="crate::child"]');
  childCluster.children[0].fire("pointerover");
  assert(page.region("crate::child").classList.contains("is-selected"));
  assert(!page.region("crate").classList.contains("is-selected"));
  page["target-select"].value = "1";
  page["target-select"].fire("change");
  assert.match(page.status.textContent, /app_lib \/ test/);
});

test("keyboard focus highlights, Enter opens a crate, and Space pins it", () => {
  for (const key of ["Enter", " "]) {
    const page = harness();
    const node = page.node("app");
    assert.equal(node.attributes.tabindex, "0");
    assert.equal(node.attributes.role, "button");
    node.fire("focusin");
    assert(page.node("core").classList.contains("is-outgoing"));
    const event = node.fire("keydown", { key });
    assert(event.defaultPrevented);
    if (key === "Enter") {
      assert(page.world.classList.contains("modules"));
      page.region("crate::child").fire("focusin");
      assert(page.region("crate::child").classList.contains("is-selected"));
    } else {
      assert(page.world.classList.contains("crates"));
      node.fire("focusout");
      assert(node.classList.contains("is-selected"));
      assert.match(page.status.textContent, /^Pinned/);
    }
  }
});

test("drag is unbounded, suppresses the following click and preserves the crate camera on back", () => {
  const page = harness();
  const node = page.node("app");
  const initial = page.world.style.transform;
  node.fire("pointerdown", { clientX: 10, clientY: 10 });
  page.canvas.fire("pointermove", { clientX: 100010, clientY: -99990 });
  assert(page.canvas.classList.contains("dragging"));
  assert(page.canvas.hasPointerCapture(1));
  assert.notEqual(page.world.style.transform, initial);
  assert.match(page.world.style.transform, /100[0-9]{3}/);
  const moved = page.world.style.transform;
  page.canvas.fire("pointerup");
  assert(!page.canvas.hasPointerCapture(1));
  node.fire("click");
  assert(page.world.classList.contains("crates"));
  node.fire("pointerdown", { clientX: 10, clientY: 10 });
  node.fire("pointerup", { clientX: 10, clientY: 10 });
  node.fire("click");
  assert(page.world.classList.contains("crates"));
  node.fire("dblclick");
  assert(page.world.classList.contains("modules"));
  page.back.fire("click");
  assert(page.world.classList.contains("crates"));
  assert.equal(page.world.style.transform, moved);
});

test("wheel zoom stays anchored under the cursor and fit remains available", () => {
  const page = harness();
  const camera = () => page.world.style.transform.match(/translate\(([-.\d]+)px, ([-.\d]+)px\) scale\(([-.\d]+)\)/).slice(1).map(Number);
  const before = camera();
  const cursor = [410, 330];
  const event = page.canvas.fire("wheel", { clientX: cursor[0], clientY: cursor[1], deltaY: -150 });
  const after = camera();
  assert(event.defaultPrevented);
  assert(after[2] > before[2]);
  for (let index = 0; index < 2; index++) assert(Math.abs((cursor[index] - before[index]) / before[2] - (cursor[index] - after[index]) / after[2]) < 1e-8);
  page.fit.fire("click");
  assert.deepEqual(camera(), before);
});

test("unavailable diagrams and notices remain navigable and use plain text", () => {
  const page = harness({ notices: ["<script>metadata</script>"] });
  assert.equal(page["notice-list"].children[0].textContent, "<script>metadata</script>");
  page.node("core").fire("dblclick");
  assert.equal(page.empty.hidden, false);
  assert.equal(page["target-control"].hidden, true);
  assert.match(page["empty-message"].textContent, /core/);
  page.back.fire("click");
  assert(page.node("app"));
  const missing = harness({ crates: null });
  assert.equal(missing.empty.hidden, false);
  assert.match(missing["empty-message"].textContent, /Could not generate the crate graph/);
});

test("breadcrumb links navigate to crate, package and target levels", () => {
  const page = harness();
  const crumbs = () => page["view-path"].querySelectorAll("a");
  assert.deepEqual(crumbs().map((link) => link.textContent), ["Crates"]);
  page.node("app").fire("dblclick");
  page["target-select"].value = "1";
  page["target-select"].fire("change");
  assert.deepEqual(crumbs().map((link) => link.textContent), ["Crates", "app", "app_lib (lib, test)"]);
  assert.equal(crumbs()[2].attributes["aria-current"], "page");
  const event = crumbs()[1].fire("click");
  assert(event.defaultPrevented);
  assert.equal(page["target-select"].value, "2");
  assert.match(page.status.textContent, /app_lib \/ normal/);
  crumbs()[2].fire("click");
  assert(page.world.classList.contains("modules"));
  crumbs()[0].fire("click");
  assert(page.world.classList.contains("crates"));
  assert.equal(page.back.hidden, true);
  assert.equal(crumbs().length, 1);
  page.node("core").fire("dblclick");
  assert.equal(crumbs()[1].attributes["aria-current"], "page");
  crumbs()[0].fire("click");
  assert(page.node("app"));
});

test("single-click pins crate dependencies across hover, canvas pan, zoom and fit; background clears", () => {
  const page = harness();
  const app = page.node("app");
  app.fire("pointerover");
  app.fire("click");
  assert(page.world.classList.contains("crates"));
  assert.equal(app.attributes["aria-pressed"], "true");
  app.fire("pointerout");
  page.node("core").fire("pointerover");
  assert(app.classList.contains("is-selected"));
  assert(!page.node("core").classList.contains("is-selected"));
  assert(page.edge("out").classList.contains("is-outgoing"));
  page.canvas.fire("pointerdown", { clientX: 100, clientY: 100 });
  page.canvas.fire("pointermove", { clientX: 5000, clientY: -5000 });
  page.canvas.fire("pointerup");
  page.canvas.fire("click");
  assert.match(page.status.textContent, /^Pinned · app/);
  page["zoom-in"].fire("click");
  page.fit.fire("click");
  assert(app.classList.contains("is-selected"));
  page.canvas.fire("pointerdown");
  page.canvas.fire("pointerup");
  page.canvas.fire("click");
  assert(!page.world.classList.contains("has-focus"));
  assert.equal(app.attributes["aria-pressed"], "false");
  assert(!page.edge("out").classList.contains("is-outgoing"));
  page.node("support").fire("pointerover");
  assert(page.node("support").classList.contains("is-selected"));
});

test("module labels pin exact modules and pins can be replaced or cleared", () => {
  const page = harness();
  page.node("app").fire("click");
  // Native double-click delivers two clicks before dblclick.
  page.node("app").fire("click");
  page.node("app").fire("dblclick");
  assert(page.world.classList.contains("modules"));
  assert(!page.world.classList.contains("has-focus"));
  const leaf = page.region("crate::child::leaf");
  leaf.children[1].fire("click");
  leaf.fire("pointerout");
  page.region("crate").fire("pointerover");
  assert(leaf.classList.contains("is-selected"));
  assert(page.edge("leaf-child").classList.contains("is-outgoing"));
  page.node("crate::child").fire("click");
  assert(page.region("crate::child").classList.contains("is-selected"));
  assert(!leaf.classList.contains("is-selected"));
  page.canvas.fire("keydown", { key: "Escape" });
  assert(!page.world.classList.contains("has-focus"));
  page.region("crate").fire("click");
  page.world.firstElementChild.fire("click");
  assert(!page.world.classList.contains("has-focus"));
});

test("persistent tree selects crates and modules across different graphs without rebuilding branches", () => {
  const page = harness();
  const root = page.tree.firstElementChild;
  const toggles = page.tree.querySelectorAll(".tree-toggle");
  const appTargetToggle = page.treeNode("module:2", "crate").parentElement.children[0];
  const appTargetChildren = page.treeNode("module:2", "crate").parentElement.parentElement.children[1];
  appTargetToggle.fire("click");
  assert.equal(appTargetChildren.hidden, false);
  page.tree.scrollTop = 120;
  page.treeNode("crates", "app").fire("pointerover");
  assert(page.edge("out").classList.contains("is-outgoing"));
  assert(page.treeNode("crates", "core").classList.contains("is-outgoing"));
  page.treeNode("module:2", "crate::child::leaf").fire("click");
  assert.equal(page["target-select"].value, "2");
  assert(page.region("crate::child::leaf").classList.contains("is-selected"));
  assert.match(page.status.textContent, /^Pinned/);
  page.treeNode("crates", "support").fire("click");
  assert(page.world.classList.contains("crates"));
  assert(page.node("support").classList.contains("is-selected"));
  page.treeNode("crates", "support").fire("dblclick");
  assert(page.world.classList.contains("modules"));
  assert.match(page.status.textContent, /support \/ normal/);
  page.treeNode("module:1", "crate::child").fire("click");
  assert.equal(page["target-select"].value, "1");
  assert(page.region("crate::child").classList.contains("is-selected"));
  assert.match(page.status.textContent, /Pinned/);
  assert.equal(page.tree.firstElementChild, root);
  assert.deepEqual(page.tree.querySelectorAll(".tree-toggle"), toggles);
  assert.equal(appTargetChildren.hidden, false);
  assert.equal(page.tree.scrollTop, 120);
  page.tree.fire("click");
  assert(!page.world.classList.contains("has-focus"));
  assert.equal(page.treeNode("module:1", "crate::child").attributes["aria-pressed"], "false");
});

test("tree hover is scoped to its graph, keyboard pins and opens, and missing renderers retain packages", () => {
  const page = harness();
  page.node("app").fire("dblclick");
  page.treeNode("module:3", "crate::child::leaf").fire("pointerover");
  assert(page.treeNode("module:3", "crate").classList.contains("is-incoming"));
  assert(!page.region("crate").classList.contains("is-incoming"));
  assert(!page.world.classList.contains("has-focus"));
  const selected = page.treeNode("module:3", "crate::child::leaf");
  selected.fire("keydown", { key: " " });
  assert(page.region("crate::child::leaf").classList.contains("is-selected"));
  selected.fire("focusout");
  assert.match(page.status.textContent, /^Pinned/);
  selected.fire("keydown", { key: "Escape" });
  assert(!page.world.classList.contains("has-focus"));
  page.treeNode("crates", "app").fire("keydown", { key: "Enter" });
  assert.match(page.status.textContent, /app_lib \/ normal/);
  const missing = harness({ crates: null, modules: [], packages: ["failed", "gated"] });
  assert(missing.treeNode("crates", "failed"));
  assert(missing.treeNode("crates", "gated"));
  missing.treeNode("crates", "failed").fire("dblclick");
  assert.equal(missing.empty.hidden, false);
});
