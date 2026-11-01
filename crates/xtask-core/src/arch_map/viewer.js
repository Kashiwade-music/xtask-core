(() => {
  "use strict";
  const data = JSON.parse(document.getElementById("architecture-data").textContent);
  const get = (id) => document.getElementById(id);
  const canvas = get("canvas");
  const world = get("world");
  const select = get("target-select");
  const tree = get("tree");
  const views = new Map();
  const graphIndex = new Map();
  const treeRows = [];
  let key = "crates";
  let currentPackage = null;
  let camera = { x: 0, y: 0, scale: 1 };
  let size = { width: 1, height: 1 };
  let nodes = [], regions = [], edges = [], labels = [];
  let hovered = null, focused = null, pinned = null, drag = null, suppressClick = false;
  let defaultStatus = "";
  const minScale = 0.02, maxScale = 8;
  const libraryKinds = new Set(["lib", "rlib", "staticlib", "cdylib", "dylib", "proc-macro"]);

  function paintCamera() {
    world.style.transform = `translate(${camera.x}px, ${camera.y}px) scale(${camera.scale})`;
    get("zoom-level").textContent = `${Math.round(camera.scale * 100)}%`;
    views.set(key, { ...camera });
  }

  function fit() {
    if (!world.firstElementChild) return;
    const width = Math.max(100, canvas.clientWidth - 100);
    const height = Math.max(100, canvas.clientHeight - 150);
    camera.scale = Math.min(1, Math.max(minScale, Math.min(width / size.width, height / size.height)));
    camera.x = (canvas.clientWidth - size.width * camera.scale) / 2;
    camera.y = 96 + (height - size.height * camera.scale) / 2;
    paintCamera();
  }

  function zoom(factor, x = canvas.clientWidth / 2, y = canvas.clientHeight / 2) {
    const next = Math.min(maxScale, Math.max(minScale, camera.scale * factor));
    const ratio = next / camera.scale;
    camera.x = x - (x - camera.x) * ratio;
    camera.y = y - (y - camera.y) * ratio;
    camera.scale = next;
    paintCamera();
  }

  function entity(element) {
    if (!(element instanceof Element) || !world.contains(element)) return null;
    const item = element.closest(".node[data-node-id], .region-label[data-region-id], .cluster[data-region-id]");
    return item ? (item.dataset.nodeId ?? item.dataset.regionId) : null;
  }

  function classify(element, selected, incoming, outgoing, active) {
    element.classList.toggle("is-selected", selected);
    element.classList.toggle("is-incoming", !selected && incoming);
    element.classList.toggle("is-outgoing", !selected && outgoing);
    element.classList.toggle("is-dim", active && !selected && !incoming && !outgoing);
  }

  function highlight() {
    const selected = pinned ?? hovered ?? focused;
    const id = selected?.view === key ? selected.id : null;
    const graph = graphIndex.get(selected?.view);
    const incoming = new Set(), outgoing = new Set(), activeEdges = new Map();
    for (const edge of graph?.edges ?? []) {
      if (edge.to === selected.id) incoming.add(edge.from);
      if (edge.from === selected.id) outgoing.add(edge.to);
    }
    for (const edge of edges) {
      const isIncoming = id !== null && edge.dataset.to === id;
      const isOutgoing = id !== null && edge.dataset.from === id;
      classify(edge, false, isIncoming, isOutgoing, id !== null);
      activeEdges.set(edge.dataset.edgeId, [isIncoming, isOutgoing]);
    }
    for (const item of [...nodes, ...regions]) {
      const nodeId = item.dataset.nodeId ?? item.dataset.regionId;
      classify(item, id !== null && nodeId === id, id !== null && incoming.has(nodeId), id !== null && outgoing.has(nodeId), id !== null);
      if (!item.classList.contains("cluster")) item.setAttribute("aria-pressed", String(pinned?.view === key && pinned.id === nodeId));
    }
    for (const label of labels) {
      const [isIncoming, isOutgoing] = activeEdges.get(label.dataset.edgeId) ?? [false, false];
      classify(label, false, isIncoming, isOutgoing, id !== null);
    }
    world.classList.toggle("has-focus", id !== null);
    for (const row of treeRows) {
      const sameGraph = row.view === selected?.view;
      classify(row.element, sameGraph && row.id === selected.id, sameGraph && incoming.has(row.id), sameGraph && outgoing.has(row.id), false);
      row.element.setAttribute("aria-pressed", String(row.view === pinned?.view && row.id === pinned.id));
    }
    get("status").textContent = selected === null ? defaultStatus : `${pinned ? "Pinned · " : ""}${selected.id}  ·  ${incoming.size} dependents  ·  ${outgoing.size} dependencies`;
  }

  function indexGraph(view, source) {
    if (!source) return;
    const svg = new DOMParser().parseFromString(source, "image/svg+xml").documentElement;
    if (svg.localName !== "svg") return;
    const parents = new Map([...svg.querySelectorAll(".cluster[data-region-id]")].map((region) => [region.dataset.regionId, region.dataset.parent || null]));
    graphIndex.set(view, {
      nodes: [...svg.querySelectorAll(".node[data-node-id]")].map((node) => ({ id: node.dataset.nodeId, parent: parents.get(node.dataset.nodeId) ?? null })),
      edges: [...svg.querySelectorAll(".edge[data-edge-id]")].map((edge) => ({ from: edge.dataset.from, to: edge.dataset.to })),
    });
  }

  function treeReference(element) {
    if (!(element instanceof Element) || !tree.contains(element)) return null;
    const row = element.closest(".tree-entity");
    return row ? { view: row.dataset.view, id: row.dataset.nodeId } : null;
  }

  function buildTree() {
    // Build once. Navigation only updates row highlights; expanded branches and
    // scroll position stay independent of the current canvas graph.
    const root = document.createElement("ul");
    const branch = (parent, label, ref, children, open = false) => {
      const entry = document.createElement("li");
      const row = document.createElement("div");
      row.className = "tree-row";
      const toggle = document.createElement("button");
      toggle.className = "tree-toggle";
      toggle.setAttribute("type", "button");
      const list = document.createElement("ul");
      list.hidden = !open;
      if (children.length) {
        toggle.textContent = open ? "▾" : "▸";
        toggle.setAttribute("aria-label", `Expand or collapse ${label}`);
        toggle.setAttribute("aria-expanded", String(open));
        toggle.addEventListener("click", () => {
          list.hidden = !list.hidden;
          toggle.textContent = list.hidden ? "▸" : "▾";
          toggle.setAttribute("aria-expanded", String(!list.hidden));
        });
      } else {
        toggle.hidden = true;
        row.classList.add("tree-leaf");
      }
      const button = document.createElement("button");
      button.className = "tree-entity";
      button.textContent = label;
      button.setAttribute("type", "button");
      button.setAttribute("data-view", ref.view);
      button.setAttribute("data-node-id", ref.id);
      button.setAttribute("aria-pressed", "false");
      button.setAttribute("title", `${ref.id}: click to pin${ref.view === "crates" ? "; double-click or Enter to open module graph" : ""}`);
      treeRows.push({ element: button, ...ref });
      row.append(toggle, button);
      entry.append(row, list);
      parent.append(entry);
      for (const child of children) child(list);
    };
    const packages = new Set([...(data.packages ?? []), ...(graphIndex.get("crates")?.nodes.map((node) => node.id) ?? []), ...data.modules.map((item) => item.package)]);
    for (const packageName of [...packages].sort()) {
      const targets = data.modules.flatMap((item, index) => item.package === packageName ? [index] : []);
      const targetBranches = targets.map((index) => (parent) => {
        const item = data.modules[index];
        const view = `module:${index}`;
        const entries = graphIndex.get(view)?.nodes ?? [];
        const ids = new Set(entries.map((node) => node.id));
        const children = (parentId) => entries.filter((node) => (ids.has(node.parent) ? node.parent : null) === parentId)
          .sort((a, b) => a.id.localeCompare(b.id)).map((node) => (list) => {
            branch(list, node.id.split("::").at(-1), { view, id: node.id }, children(node.id));
          });
        const rootId = entries.find((node) => !ids.has(node.parent))?.id ?? "crate";
        branch(parent, `${item.target} (${item.kind}, ${item.test ? "test" : "normal"})`, { view, id: rootId }, children(null));
      });
      branch(root, packageName, { view: "crates", id: packageName }, targetBranches, true);
    }
    tree.append(root);
  }

  function clearSelection() {
    pinned = hovered = focused = null;
    highlight();
  }

  function pin(ref) {
    if (ref.view !== key) {
      if (ref.view === "crates") showCrates();
      else {
        const index = Number(ref.view.slice(7));
        openPackage(data.modules[index].package, index);
      }
    }
    pinned = ref;
    hovered = focused = null;
    highlight();
  }

  function preferred(indices) {
    const rank = (index) => {
      const item = data.modules[index];
      const kinds = item.kind.split("+");
      return (item.test ? 10 : 0) + (kinds.some((kind) => libraryKinds.has(kind)) ? 0 : kinds.includes("bin") ? 1 : 2);
    };
    return [...indices].sort((a, b) => rank(a) - rank(b) || a - b)[0];
  }

  function openPackage(packageName, selectedIndex) {
    const indices = data.modules.flatMap((item, index) => item.package === packageName ? [index] : []);
    currentPackage = packageName;
    select.replaceChildren();
    for (const index of indices) {
      const item = data.modules[index];
      const option = document.createElement("option");
      option.value = String(index);
      option.textContent = `${item.target} · ${item.kind} · ${item.test ? "test" : "normal"}`;
      select.append(option);
    }
    const index = selectedIndex ?? preferred(indices);
    if (index !== undefined) select.value = String(index);
    show(index === undefined ? `missing:${packageName}` : `module:${index}`);
  }

  function showCrates() {
    currentPackage = null;
    show("crates");
  }

  function breadcrumbs(item) {
    const path = get("view-path");
    path.replaceChildren();
    const crumb = (text, href, navigate, current) => {
      if (path.children.length) {
        const separator = document.createElement("span");
        separator.textContent = "/";
        separator.setAttribute("aria-hidden", "true");
        path.append(separator);
      }
      const link = document.createElement("a");
      link.textContent = text;
      link.setAttribute("href", href);
      if (current) link.setAttribute("aria-current", "page");
      link.addEventListener("click", (event) => { event.preventDefault(); navigate(); });
      path.append(link);
    };
    crumb("Crates", "#crates", showCrates, key === "crates");
    if (currentPackage !== null) {
      const packageName = currentPackage;
      crumb(packageName, `#package/${encodeURIComponent(packageName)}`, () => openPackage(packageName), !item);
    }
    if (item) {
      const targetKey = key;
      crumb(`${item.target} (${item.kind}, ${item.test ? "test" : "normal"})`, `#${targetKey}`, () => show(targetKey), true);
    }
  }

  function show(nextKey) {
    if (drag) finishDrag();
    key = nextKey;
    const isCrates = key === "crates";
    const item = key.startsWith("module:") ? data.modules[Number(key.slice(7))] : null;
    const svgText = isCrates ? data.crates : item?.svg;
    get("back").hidden = isCrates;
    get("target-control").hidden = isCrates || !select.options.length;
    breadcrumbs(item);
    world.replaceChildren();
    world.className = isCrates ? "crates" : "modules";
    hovered = focused = pinned = null;
    nodes = []; regions = []; edges = []; labels = [];
    get("empty").hidden = Boolean(svgText);
    if (!svgText) {
      get("empty-message").textContent = isCrates ? "Could not generate the crate graph. Check the generation notices." : `No module diagrams for ${currentPackage}. Check the diagram targets, required features, and analysis results.`;
      defaultStatus = "No diagram available";
      get("status").textContent = defaultStatus;
      highlight();
      return;
    }
    // The generator supplies the already escaped, trusted SVG renderer output.
    const documentSvg = new DOMParser().parseFromString(svgText, "image/svg+xml");
    const svg = document.importNode(documentSvg.documentElement, true);
    if (svg.localName !== "svg") {
      get("empty").hidden = false;
      get("empty-message").textContent = "Could not load the SVG. Generate the diagrams again.";
      defaultStatus = "Could not load SVG";
      highlight();
      return;
    }
    svg.setAttribute("role", "group");
    svg.setAttribute("aria-label", isCrates ? "Crate dependencies" : `Module dependencies for ${currentPackage}`);
    world.append(svg);
    const box = svg.viewBox.baseVal;
    size = { width: box.width || svg.width.baseVal.value || 1, height: box.height || svg.height.baseVal.value || 1 };
    nodes = [...svg.querySelectorAll(".node[data-node-id]")];
    regions = [...svg.querySelectorAll(".cluster[data-region-id], .region-label[data-region-id]")];
    edges = [...svg.querySelectorAll(".edge[data-edge-id]")];
    labels = [...svg.querySelectorAll(".edge-label[data-edge-id]")];
    for (const node of nodes) {
      node.setAttribute("tabindex", "0");
      node.setAttribute("role", "button");
      node.setAttribute("aria-label", `${node.dataset.nodeId}: click or Space to pin dependencies${isCrates ? "; double-click or Enter to open module graph" : ""}`);
    }
    for (const label of svg.querySelectorAll(".region-label[data-region-id]")) {
      label.setAttribute("tabindex", "0");
      label.setAttribute("role", "button");
      label.setAttribute("aria-label", label.dataset.regionId);
    }
    defaultStatus = `${nodes.length} ${isCrates ? "crates" : "modules"}  ·  ${edges.length} dependencies${item ? `  ·  ${item.target} / ${item.test ? "test" : "normal"}` : ""}`;
    highlight();
    if (views.has(key)) { camera = { ...views.get(key) }; paintCamera(); }
    else fit();
  }

  function finishDrag() {
    if (!drag) return;
    if (canvas.hasPointerCapture(drag.pointerId)) canvas.releasePointerCapture(drag.pointerId);
    suppressClick = drag.moved;
    drag = null;
    canvas.classList.remove("dragging");
  }

  canvas.addEventListener("pointerdown", (event) => {
    if (event.button !== 0 || event.target.closest("button, .zoom-controls, .guide, .empty")) return;
    suppressClick = false;
    drag = { pointerId: event.pointerId, startX: event.clientX, startY: event.clientY, x: camera.x, y: camera.y, moved: false };
  });
  canvas.addEventListener("pointermove", (event) => {
    if (!drag || event.pointerId !== drag.pointerId) return;
    const dx = event.clientX - drag.startX, dy = event.clientY - drag.startY;
    if (!drag.moved && Math.hypot(dx, dy) < 5) return;
    if (!drag.moved) {
      drag.moved = true;
      canvas.setPointerCapture(event.pointerId);
      canvas.classList.add("dragging");
      hovered = focused = null;
      highlight();
    }
    camera.x = drag.x + dx;
    camera.y = drag.y + dy;
    paintCamera();
  });
  canvas.addEventListener("pointerup", finishDrag);
  canvas.addEventListener("pointercancel", finishDrag);
  canvas.addEventListener("lostpointercapture", finishDrag);
  window.addEventListener("pointerup", finishDrag);
  const canvasReference = (element) => {
    const id = entity(element);
    return id === null ? null : { view: key, id };
  };
  world.addEventListener("pointerover", (event) => { if (!drag?.moved) { hovered = canvasReference(event.target); highlight(); } });
  world.addEventListener("pointerout", (event) => { if (!drag?.moved) { hovered = canvasReference(event.relatedTarget); highlight(); } });
  world.addEventListener("focusin", (event) => { focused = canvasReference(event.target); highlight(); });
  world.addEventListener("focusout", (event) => { focused = canvasReference(event.relatedTarget); highlight(); });
  canvas.addEventListener("click", (event) => {
    if (event.target.closest("button, .zoom-controls, .guide, .empty")) return;
    if (suppressClick) { suppressClick = false; return; }
    const ref = canvasReference(event.target);
    if (ref) pin(ref);
    else clearSelection();
  });
  world.addEventListener("dblclick", (event) => {
    const id = entity(event.target);
    if (!suppressClick && key === "crates" && id !== null) openPackage(id);
  });
  world.addEventListener("keydown", (event) => {
    if (event.key === "Enter" || event.key === " ") {
      const id = entity(event.target);
      if (id !== null) {
        event.preventDefault();
        if (key === "crates" && event.key === "Enter") openPackage(id);
        else pin({ view: key, id });
      }
    }
  });
  canvas.addEventListener("wheel", (event) => {
    if (event.target.closest("button, .zoom-controls")) return;
    event.preventDefault();
    const rect = canvas.getBoundingClientRect();
    const delta = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? canvas.clientHeight : 1);
    zoom(Math.exp(-Math.max(-500, Math.min(500, delta)) * 0.0015), event.clientX - rect.left, event.clientY - rect.top);
  }, { passive: false });
  canvas.addEventListener("keydown", (event) => {
    if (event.ctrlKey || event.metaKey || event.altKey || event.target.closest("button, select")) return;
    if (event.key.toLowerCase() === "f") { event.preventDefault(); fit(); }
    if (event.key === "+" || event.key === "=") { event.preventDefault(); zoom(1.25); }
    if (event.key === "-") { event.preventDefault(); zoom(0.8); }
    if (event.key === "Escape") clearSelection();
    const steps = { ArrowLeft: [60, 0], ArrowRight: [-60, 0], ArrowUp: [0, 60], ArrowDown: [0, -60] };
    if (steps[event.key]) { event.preventDefault(); camera.x += steps[event.key][0]; camera.y += steps[event.key][1]; paintCamera(); }
  });
  get("zoom-in").addEventListener("click", () => zoom(1.25));
  get("zoom-out").addEventListener("click", () => zoom(0.8));
  get("fit").addEventListener("click", fit);
  get("back").addEventListener("click", showCrates);
  tree.addEventListener("pointerover", (event) => { hovered = treeReference(event.target); highlight(); });
  tree.addEventListener("pointerout", (event) => { hovered = treeReference(event.relatedTarget); highlight(); });
  tree.addEventListener("focusin", (event) => { focused = treeReference(event.target); highlight(); });
  tree.addEventListener("focusout", (event) => { focused = treeReference(event.relatedTarget); highlight(); });
  tree.addEventListener("click", (event) => {
    if (event.target.closest(".tree-toggle")) return;
    const ref = treeReference(event.target);
    if (ref) pin(ref);
    else clearSelection();
  });
  tree.addEventListener("dblclick", (event) => {
    const ref = treeReference(event.target);
    if (ref?.view === "crates") openPackage(ref.id);
  });
  tree.addEventListener("keydown", (event) => {
    if (event.key === "Escape") { event.preventDefault(); clearSelection(); }
    const ref = treeReference(event.target);
    if (!ref || (event.key !== "Enter" && event.key !== " ")) return;
    event.preventDefault();
    if (event.key === "Enter" && ref.view === "crates") openPackage(ref.id);
    else pin(ref);
  });
  select.addEventListener("change", () => show(`module:${select.value}`));
  if (data.notices.length) {
    get("notices").hidden = false;
    get("notice-summary").textContent = `Generation notices (${data.notices.length})`;
    for (const notice of data.notices) {
      const item = document.createElement("li");
      item.textContent = notice;
      get("notice-list").append(item);
    }
  }
  indexGraph("crates", data.crates);
  data.modules.forEach((item, index) => indexGraph(`module:${index}`, item.svg));
  buildTree();
  show("crates");
})();
