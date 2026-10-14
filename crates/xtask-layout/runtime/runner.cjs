'use strict';

// Pinned elkjs is embedded beside this runner; no npm or network at runtime.
const ELK = require(process.argv[2]);
const readline = require('node:readline');
const rootId = '__xtask_root__';

async function build(request, elk) {
    const graph = {
        id: rootId,
        layoutOptions: {
            'elk.algorithm': 'layered',
            'elk.direction': request.direction === 'TopToBottom' ? 'DOWN' : 'RIGHT',
            'elk.edgeRouting': 'ORTHOGONAL',
            'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
            'elk.json.edgeCoords': 'ROOT',
            'elk.json.shapeCoords': 'ROOT',
            'elk.randomSeed': '1',
            'elk.layered.mergeEdges': 'false',
            'elk.layered.mergeHierarchyEdges': 'false',
            'elk.layered.allowNonFlowPortsToSwitchSides': 'false',
            'elk.spacing.nodeNode': '40',
            'elk.layered.spacing.nodeNodeBetweenLayers': '72',
            'elk.spacing.edgeNode': '16',
            'elk.spacing.edgeEdge': '12',
            'elk.padding': '[top=24,left=24,bottom=24,right=24]',
        },
        children: [], edges: [],
    };
    const nodes = new Map();
    const regions = new Map();
    const edgeMap = new Map();
    request.nodes.forEach((node, index) => nodes.set(node.id, {
        id: `n${index}`, width: node.width, height: node.height,
        layoutOptions: { 'elk.portConstraints': 'FIXED_SIDE' }, ports: [],
        source: node,
    }));
    function region(input) {
        const result = {
            id: `r${regions.size}`,
            layoutOptions: {
                'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
                'elk.padding': `[top=${input.label_height + 20},left=20,bottom=20,right=20]`,
                'elk.nodeLabels.placement': 'INSIDE V_TOP H_CENTER',
                'elk.nodeSize.constraints': 'MINIMUM_SIZE',
                'elk.nodeSize.minimum': `(${input.label_width + 40},${input.label_height + 40})`,
            },
            labels: [{text: input.label, width: input.label_width, height: input.label_height}],
            children: [], source: input,
        };
        regions.set(input.id, result);
        result.children = [...input.nodes.map(id => nodes.get(id)), ...input.children.map(region)];
        return result;
    }
    const columns = new Map();
    if (request.ranks.length) {
        graph.layoutOptions['elk.partitioning.activate'] = 'true';
        for (const [rank, ids] of request.ranks.entries()) {
            const ordered = [...ids].sort();
            const local = await elk.layout({id: `column${rank}`, layoutOptions: {
                'elk.algorithm': 'layered',
                'elk.direction': 'DOWN',
                'elk.padding': '[top=0,left=0,bottom=0,right=0]',
                'elk.layered.spacing.nodeNodeBetweenLayers': '40',
                'elk.randomSeed': '1',
            }, children: ordered.map(id => ({id: nodes.get(id).id,
                width: nodes.get(id).width, height: nodes.get(id).height, source: nodes.get(id).source})),
                edges: ordered.slice(1).map((id, index) => ({id: `order${index}`,
                    sources: [nodes.get(ordered[index]).id], targets: [nodes.get(id).id]}))});
            const column = {id: `column${rank}`, width: local.width, height: local.height, ports: [],
                layoutOptions: {'elk.portConstraints': 'FIXED_POS', 'elk.partitioning.partition': String(rank)}};
            columns.set(column.id, local.children);
            for (const child of local.children) nodes.set(child.source.id, {column, child});
            graph.children.push(column);
        }
        // Each column is one ELK node. Non-normal edges within a rank therefore
        // become self-loops and cannot move a child into a different layer.
        for (let rank = 1; rank < graph.children.length; rank++) graph.edges.push({
            id: `rankorder${rank}`, sources: [graph.children[rank - 1].id], targets: [graph.children[rank].id]});
    } else {
        graph.children = request.regions.map(region);
        const contained = new Set();
        function visit(input) { input.nodes.forEach(id => contained.add(id)); input.children.forEach(visit); }
        request.regions.forEach(visit);
        graph.children.push(...request.nodes.filter(node => !contained.has(node.id)).map(node => nodes.get(node.id)));
    }
    const incidents = new Map();
    request.edges.forEach(edge => {
        for (const key of [`${edge.from}:EAST`, `${edge.to}:WEST`]) incidents.set(key, (incidents.get(key) || 0) + 1);
    });
    const used = new Map();
    function port(id, side, portId) {
        const item = nodes.get(id);
        const result = {id: portId, width: 0, height: 0, layoutOptions: {'elk.port.side': side}};
        if (item.column) {
            const key = `${id}:${side}`;
            const index = (used.get(key) || 0) + 1;
            used.set(key, index);
            result.x = item.child.x + (side === 'EAST' ? item.child.width : 0);
            result.y = item.child.y + item.child.height * index / (incidents.get(key) + 1);
            item.column.ports.push(result);
        } else item.ports.push(result);
        return result.id;
    }
    request.edges.forEach((edge, index) => {
        const sourcePort = `p${index}s`;
        const targetPort = `p${index}t`;
        const result = {id: `e${index}`, sources: [port(edge.from, 'EAST', sourcePort)], targets: [port(edge.to, 'WEST', targetPort)]};
        if (edge.label !== null) {
            const metrics = request.label_metrics[edge.id];
            result.labels = [{text: edge.label, width: metrics.width, height: metrics.height}];
        }
        edgeMap.set(result.id, edge);
        graph.edges.push(result);
    });
    return {graph, nodes, regions, edgeMap, columns};
}

function scene(request, output, maps) {
    const result = {title: request.title, width: Math.max(output.width || 0, request.title_width), height: output.height || 0,
        nodes: [], regions: [], edges: []};
    function visit(node, parentRegion) {
        for (const child of maps.columns.get(node.id) || []) {
            const source = child.source;
            result.nodes.push({id: source.id, label: source.label, lines: source.lines,
                x: node.x + child.x, y: node.y + child.y, width: child.width, height: child.height});
        }
        const source = node.source;
        if (source && Object.hasOwn(source, 'nodes')) {
            result.regions.push({id: source.id, parent: parentRegion, label: source.label, lines: source.lines,
                x: node.x, y: node.y, width: node.width, height: node.height});
            parentRegion = source.id;
        } else if (source) {
            result.nodes.push({id: source.id, label: source.label, lines: source.lines,
                x: node.x, y: node.y, width: node.width, height: node.height});
        }
        for (const child of node.children || []) visit(child, parentRegion);
        for (const edge of node.edges || []) {
            const input = maps.edgeMap.get(edge.id);
            if (!input) continue;
            const sections = edge.sections || [];
            if (sections.length !== 1) throw new Error(`edge ${input.id} has ${sections.length} sections; expected one continuous ELK route`);
            const section = sections[0];
            const points = [section.startPoint, ...(section.bendPoints || []), section.endPoint];
            const metrics = request.label_metrics[input.id];
            let label = null;
            if (input.label !== null) {
                const positioned = (edge.labels || [])[0];
                if (!positioned || !Number.isFinite(positioned.x) || !Number.isFinite(positioned.y)) throw new Error(`ELK edge label ${input.id} has no position`);
                label = {text: input.label, lines: metrics.lines, width: metrics.width, height: metrics.height,
                    x: positioned.x + positioned.width / 2, y: positioned.y + positioned.height / 2};
            }
            result.edges.push({id: input.id, from: input.from, to: input.to, style: input.style, points, label});
        }
    }
    visit(output, null);
    result.nodes.sort((a,b) => a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
    result.regions.sort((a,b) => a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
    result.edges.sort((a,b) => a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
    return result;
}

async function main() {
    const lines = readline.createInterface({input: process.stdin, crlfDelay: Infinity});
    for await (const line of lines) {
        try {
            const request = JSON.parse(line);
            // Keep each graph independent while amortizing Node startup and the
            // embedded bundle's module loading across a whole workspace.
            const elk = new ELK();
            const maps = await build(request, elk);
            const output = await elk.layout(maps.graph);
            process.stdout.write(JSON.stringify({scene: scene(request, output, maps)}) + '\n');
        } catch (error) {
            process.stdout.write(JSON.stringify({error: error.stack || String(error)}) + '\n');
        }
    }
}
main().catch(error => { console.error(error.stack || String(error)); process.exitCode = 1; });
