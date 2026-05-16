import 'package:flutter/material.dart';
import 'package:provider/provider.dart';
import '../models/graph_node.dart';
import '../models/triple.dart';
import '../services/connection_provider.dart';
import '../theme/loka_theme.dart';
import '../widgets/graph_canvas.dart';

/// Main graph visualization screen.
///
/// Loads triples from Loka and renders them as an interactive
/// force-directed graph. Supports filtering by semantic/vector/all view modes.
class GraphScreen extends StatefulWidget {
  const GraphScreen({super.key});

  @override
  State<GraphScreen> createState() => _GraphScreenState();
}

class _GraphScreenState extends State<GraphScreen> {
  List<Triple> _triples = [];
  List<GraphNode> _nodes = [];
  List<GraphEdge> _edges = [];
  GraphViewMode _viewMode = GraphViewMode.all;
  String? _selectedNodeId;
  bool _loading = false;
  String? _error;
  int _limit = 50;
  Set<String> _hiddenPredicates = {};
  Set<String> _allPredicates = {};
  Map<String, String> _labelOverrides = {};

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) => _loadGraph());
  }

  Future<void> _loadGraph() async {
    final conn = context.read<ConnectionProvider>();
    if (!conn.connected) {
      setState(() => _error = 'Not connected to Loka');
      return;
    }

    setState(() {
      _loading = true;
      _error = null;
    });

    try {
      final triples = await conn.client.fetchTriples(limit: _limit);

      // Fetch Japanese labels for better node display
      final jaLabels = <String, String>{};
      try {
        final labelResult = await conn.client.query(
          'SELECT ?s ?label WHERE { ?s <http://www.w3.org/2000/01/rdf-schema#label> ?label } LIMIT 1000',
        );
        for (final row in labelResult.rows) {
          final s = (row['s'] as Map?)?['value']?.toString() ?? '';
          final label = (row['label'] as Map?)?['value']?.toString() ?? '';
          // Prefer Japanese labels, fallback to any label
          if (label.contains('@ja')) {
            jaLabels[s] = Triple.shortName(label);
          } else if (!jaLabels.containsKey(s) && label.isNotEmpty) {
            jaLabels[s] = Triple.shortName(label);
          }
        }
      } catch (_) {}

      // Also fetch HNSW neighbor edges if in vector/all mode
      if (_viewMode != GraphViewMode.semanticOnly) {
        try {
          final hnswResult = await conn.client.query(
            'SELECT ?s ?o WHERE { ?s <http://loka.dev/hnswNeighbor> ?o } LIMIT ${_limit ~/ 2}',
          );
          for (final row in hnswResult.rows) {
            final s = (row['s'] as Map?)?['value']?.toString() ?? '';
            final o = (row['o'] as Map?)?['value']?.toString() ?? '';
            if (s.isNotEmpty && o.isNotEmpty) {
              triples.add(Triple(
                subject: s,
                predicate: 'http://loka.dev/hnswNeighbor',
                object: o,
              ));
            }
          }
        } catch (_) {
          // HNSW edges may not be available
        }
      }

      _triples = triples;
      _labelOverrides = jaLabels;
      _buildGraph(triples);
      setState(() => _loading = false);
    } catch (e) {
      setState(() {
        _loading = false;
        _error = e.toString();
      });
    }
  }

  /// Expand a node: fetch its triples and add to the graph.
  Future<void> _expandNode(String nodeId) async {
    final conn = context.read<ConnectionProvider>();
    if (!conn.connected) return;

    try {
      final triples = await conn.client.fetchTriplesForSubject(nodeId);
      // Merge new triples into existing set
      final existingKeys =
          _triples.map((t) => '${t.subject}|${t.predicate}|${t.object}').toSet();
      final newTriples = triples
          .where((t) =>
              !existingKeys.contains('${t.subject}|${t.predicate}|${t.object}'))
          .toList();
      if (newTriples.isNotEmpty) {
        _triples.addAll(newTriples);
        _buildGraph(_triples);
        setState(() {});
      }
    } catch (e) {
      // Silently fail on expand errors
    }
  }

  /// Cascade-retraction: preview the dependency set, confirm, then delete
  /// the node and every generated inference that transitively cited it.
  Future<void> _retractNode(String iri, String label) async {
    final conn = context.read<ConnectionProvider>();
    if (!conn.connected) return;

    Map<String, dynamic> preview;
    try {
      preview = await conn.client.retractPreview(iri);
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Retract preview failed: $e')),
      );
      return;
    }
    if (!mounted) return;

    final total = preview['total'] as int? ?? 0;
    if (total == 0) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('Nothing to retract for this node.')),
      );
      return;
    }
    final maxDepth = preview['max_depth'] as int? ?? 0;
    final hnsw = preview['hnsw_tombstones'] as int? ?? 0;
    final byDepth = (preview['by_depth'] as List?) ?? [];
    final summary = byDepth
        .map((d) => '  depth ${d['depth']}: ${d['count']} triple(s)')
        .join('\n');

    final confirmed = await showDialog<bool>(
      context: context,
      builder: (ctx) => AlertDialog(
        title: Text('Retract "$label"?'),
        content: SingleChildScrollView(
          child: Text(
            'This DELETES $total triple(s) across the provenance cascade '
            '(max depth $maxDepth, $hnsw HNSW entr${hnsw == 1 ? "y" : "ies"}):\n\n'
            '$summary\n\n'
            "The node's own rows plus every generated inference that "
            'transitively cited it disappear. This cannot be undone.',
          ),
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            style: FilledButton.styleFrom(backgroundColor: LokaTheme.red),
            onPressed: () => Navigator.pop(ctx, true),
            child: Text('Delete $total triple(s)'),
          ),
        ],
      ),
    );
    if (confirmed != true) return;

    try {
      final result = await conn.client.retractCommit(iri);
      if (!mounted) return;
      final removed = result['removed'] as int? ?? 0;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Retracted $removed triple(s).')),
      );
      setState(() => _selectedNodeId = null);
      await _loadGraph();
    } catch (e) {
      if (!mounted) return;
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Retract failed: $e')),
      );
    }
  }

  void _buildGraph(List<Triple> triples) {
    final nodeIds = <String>{};
    final nodes = <GraphNode>[];
    final edges = <GraphEdge>[];
    final degreeCounts = <String, int>{};
    final hasVectorSet = <String>{};

    for (final t in triples) {
      final tripleType = classifyTriple(t);

      // Track vector subjects
      if (tripleType == TripleType.vector) {
        hasVectorSet.add(t.subject);
      }

      // Subject node
      if (!nodeIds.contains(t.subject)) {
        nodeIds.add(t.subject);
        nodes.add(GraphNode(
          id: t.subject,
          label: _labelOverrides[t.subject] ?? Triple.shortName(t.subject),
          type: t.subject.startsWith('_:')
              ? NodeType.blankNode
              : NodeType.entity,
        ));
      }
      degreeCounts[t.subject] = (degreeCounts[t.subject] ?? 0) + 1;

      // Object node (skip vector literals)
      if (!t.isVector) {
        final NodeType objType = t.object.startsWith('"')
            ? NodeType.literal
            : t.object.startsWith('_:')
                ? NodeType.blankNode
                : NodeType.entity;
        if (!nodeIds.contains(t.object)) {
          nodeIds.add(t.object);
          nodes.add(GraphNode(
            id: t.object,
            label: _labelOverrides[t.object] ?? Triple.shortName(t.object),
            type: objType,
          ));
        }
        degreeCounts[t.object] = (degreeCounts[t.object] ?? 0) + 1;

        // Edge
        EdgeType edgeType;
        switch (tripleType) {
          case TripleType.vector:
            edgeType = EdgeType.vector;
          case TripleType.hnswEdge:
            edgeType = EdgeType.hnswNeighbor;
          case TripleType.semantic:
            edgeType = EdgeType.semantic;
        }

        edges.add(GraphEdge(
          sourceId: t.subject,
          targetId: t.object,
          label: Triple.shortName(t.predicate),
          type: edgeType,
        ));
      }
    }

    // Apply degree and vector flags
    for (final node in nodes) {
      node.degree = degreeCounts[node.id] ?? 0;
      if (hasVectorSet.contains(node.id)) {
        // Mark via constructor — we rebuild anyway
      }
    }

    // Collect all unique predicates for filtering
    _allPredicates = edges.map((e) => e.label).toSet();

    // Apply predicate filter
    final filteredEdges = edges
        .where((e) => !_hiddenPredicates.contains(e.label))
        .toList();

    _nodes = nodes;
    _edges = filteredEdges;
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        // Toolbar
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
          decoration: const BoxDecoration(
            color: LokaTheme.surface,
            border: Border(bottom: BorderSide(color: LokaTheme.border)),
          ),
          child: Row(
            children: [
              const Icon(Icons.hub, size: 18, color: LokaTheme.accent),
              const SizedBox(width: 8),
              const Text('Graph',
                  style: TextStyle(
                      fontWeight: FontWeight.w600, color: LokaTheme.text)),
              const Spacer(),

              // View mode toggle
              SegmentedButton<GraphViewMode>(
                segments: const [
                  ButtonSegment(
                    value: GraphViewMode.all,
                    label: Text('All', style: TextStyle(fontSize: 11)),
                    icon: Icon(Icons.layers, size: 14),
                  ),
                  ButtonSegment(
                    value: GraphViewMode.semanticOnly,
                    label: Text('Semantic', style: TextStyle(fontSize: 11)),
                    icon: Icon(Icons.schema, size: 14),
                  ),
                  ButtonSegment(
                    value: GraphViewMode.vectorOnly,
                    label: Text('Vector', style: TextStyle(fontSize: 11)),
                    icon: Icon(Icons.grain, size: 14),
                  ),
                ],
                selected: {_viewMode},
                onSelectionChanged: (s) =>
                    setState(() => _viewMode = s.first),
                style: ButtonStyle(
                  visualDensity: VisualDensity.compact,
                  textStyle: WidgetStatePropertyAll(
                      TextStyle(fontSize: 11)),
                ),
              ),

              const SizedBox(width: 12),

              // Limit selector
              DropdownButton<int>(
                value: _limit,
                items: [25, 50, 100, 250, 500]
                    .map((v) => DropdownMenuItem(
                        value: v,
                        child: Text('$v triples',
                            style: const TextStyle(fontSize: 12))))
                    .toList(),
                onChanged: (v) {
                  if (v != null) {
                    _limit = v;
                    _loadGraph();
                  }
                },
                underline: const SizedBox(),
                dropdownColor: LokaTheme.surface,
              ),

              const SizedBox(width: 8),
              // Predicate filter
              if (_allPredicates.isNotEmpty)
                PopupMenuButton<String>(
                  icon: const Icon(Icons.filter_list, size: 18),
                  tooltip: 'Filter predicates',
                  itemBuilder: (_) => _allPredicates.map((p) {
                    final hidden = _hiddenPredicates.contains(p);
                    return PopupMenuItem<String>(
                      value: p,
                      child: Row(
                        children: [
                          Icon(
                            hidden ? Icons.check_box_outline_blank : Icons.check_box,
                            size: 16,
                            color: hidden ? LokaTheme.muted : LokaTheme.accent,
                          ),
                          const SizedBox(width: 8),
                          Text(p, style: const TextStyle(fontSize: 12)),
                        ],
                      ),
                    );
                  }).toList(),
                  onSelected: (pred) {
                    setState(() {
                      if (_hiddenPredicates.contains(pred)) {
                        _hiddenPredicates.remove(pred);
                      } else {
                        _hiddenPredicates.add(pred);
                      }
                      _buildGraph(_triples);
                    });
                  },
                ),
              const SizedBox(width: 8),
              IconButton(
                icon: const Icon(Icons.refresh, size: 18),
                tooltip: 'Reload',
                onPressed: _loadGraph,
              ),
              IconButton(
                icon: const Icon(Icons.image, size: 18),
                tooltip: 'Export graph as PNG',
                onPressed: () {
                  ScaffoldMessenger.of(context).showSnackBar(
                    const SnackBar(content: Text('Graph export: use screenshot tool (Win+Shift+S) on the graph canvas')),
                  );
                },
              ),
            ],
          ),
        ),

        // Main area: triple list + graph
        Expanded(
          child: Row(
            children: [
              // Left panel: triple list
              if (_triples.isNotEmpty)
                SizedBox(
                  width: 280,
                  child: Container(
                    decoration: const BoxDecoration(
                      color: LokaTheme.surface,
                      border: Border(right: BorderSide(color: LokaTheme.border)),
                    ),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Padding(
                          padding: const EdgeInsets.all(8),
                          child: Text(
                            '${_triples.length} triples',
                            style: const TextStyle(color: LokaTheme.muted, fontSize: 11),
                          ),
                        ),
                        Expanded(
                          child: ListView.builder(
                            itemCount: _triples.length,
                            itemBuilder: (ctx, i) {
                              final t = _triples[i];
                              return InkWell(
                                onTap: () => setState(() => _selectedNodeId = t.subject),
                                child: Padding(
                                  padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 3),
                                  child: RichText(
                                    overflow: TextOverflow.ellipsis,
                                    text: TextSpan(
                                      style: const TextStyle(fontSize: 11),
                                      children: [
                                        TextSpan(
                                          text: Triple.shortName(t.subject),
                                          style: const TextStyle(color: LokaTheme.accent),
                                        ),
                                        const TextSpan(text: ' '),
                                        TextSpan(
                                          text: Triple.shortName(t.predicate),
                                          style: const TextStyle(color: LokaTheme.purple),
                                        ),
                                        const TextSpan(text: ' '),
                                        TextSpan(
                                          text: Triple.shortName(t.object),
                                          style: TextStyle(
                                            color: t.object.startsWith('"')
                                                ? LokaTheme.green
                                                : LokaTheme.text,
                                          ),
                                        ),
                                      ],
                                    ),
                                  ),
                                ),
                              );
                            },
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              // Graph canvas
              Expanded(
          child: _loading
              ? const Center(child: CircularProgressIndicator())
              : _error != null
                  ? Center(
                      child: Column(
                        mainAxisSize: MainAxisSize.min,
                        children: [
                          Icon(Icons.error_outline,
                              color: LokaTheme.red, size: 48),
                          const SizedBox(height: 8),
                          Text(_error!,
                              style:
                                  const TextStyle(color: LokaTheme.muted)),
                          const SizedBox(height: 12),
                          ElevatedButton(
                            onPressed: _loadGraph,
                            child: const Text('Retry'),
                          ),
                        ],
                      ),
                    )
                  : Row(
                      children: [
                        // Graph canvas
                        Expanded(
                          child: GraphCanvas(
                            nodes: _nodes,
                            edges: _edges,
                            viewMode: _viewMode,
                            onNodeSelected: (id) =>
                                setState(() => _selectedNodeId = id),
                            onNodeDoubleTap: (id) => _expandNode(id),
                          ),
                        ),
                        // Detail panel
                        if (_selectedNodeId != null)
                          _buildDetailPanel(),
                      ],
                    ),
              ), // Expanded (graph canvas)
            ],
          ), // Row
        ), // Expanded (main area)

        // Status bar
        Container(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 4),
          decoration: const BoxDecoration(
            color: LokaTheme.surface,
            border: Border(top: BorderSide(color: LokaTheme.border)),
          ),
          child: Row(
            children: [
              Text(
                '${_nodes.length} nodes, ${_edges.length} edges '
                '(${_triples.length} triples loaded)',
                style: const TextStyle(
                    color: LokaTheme.muted, fontSize: 11),
              ),
              const Spacer(),
              Text(
                'Scroll to zoom, drag to pan, click nodes to inspect',
                style: const TextStyle(
                    color: LokaTheme.muted, fontSize: 11),
              ),
            ],
          ),
        ),
      ],
    );
  }

  Widget _buildDetailPanel() {
    final node = _nodes.where((n) => n.id == _selectedNodeId).firstOrNull;
    if (node == null) return const SizedBox();

    final relatedTriples =
        _triples.where((t) => t.subject == node.id || t.object == node.id);

    return Container(
      width: 280,
      decoration: const BoxDecoration(
        color: LokaTheme.surface,
        border: Border(left: BorderSide(color: LokaTheme.border)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          // Header
          Padding(
            padding: const EdgeInsets.all(12),
            child: Row(
              children: [
                Expanded(
                  child: Text(
                    node.label,
                    style: const TextStyle(
                      fontWeight: FontWeight.w600,
                      color: LokaTheme.accent,
                    ),
                  ),
                ),
                IconButton(
                  icon: const Icon(Icons.close, size: 16),
                  onPressed: () =>
                      setState(() => _selectedNodeId = null),
                ),
              ],
            ),
          ),
          const Divider(height: 1, color: LokaTheme.border),

          // Full IRI
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 6),
            child: SelectableText(
              node.id,
              style: const TextStyle(
                  color: LokaTheme.muted, fontSize: 11),
            ),
          ),

          // Stats
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12),
            child: Wrap(
              spacing: 8,
              children: [
                Chip(
                  label: Text('${node.degree} connections'),
                  visualDensity: VisualDensity.compact,
                ),
                if (node.hasVector)
                  const Chip(
                    label: Text('Has vector'),
                    avatar: Icon(Icons.grain, size: 14,
                        color: LokaTheme.purple),
                    visualDensity: VisualDensity.compact,
                  ),
              ],
            ),
          ),

          const SizedBox(height: 12),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 12),
            child: SizedBox(
              width: double.infinity,
              child: OutlinedButton.icon(
                icon: const Icon(Icons.cut, size: 16),
                label: const Text('Retract (cascade)'),
                style: OutlinedButton.styleFrom(
                  foregroundColor: LokaTheme.red,
                  side: const BorderSide(color: LokaTheme.red),
                ),
                onPressed: () => _retractNode(node.id, node.label),
              ),
            ),
          ),

          const SizedBox(height: 8),
          const Padding(
            padding: EdgeInsets.symmetric(horizontal: 12),
            child: Text('Related triples',
                style: TextStyle(
                    color: LokaTheme.muted,
                    fontSize: 11,
                    fontWeight: FontWeight.w600)),
          ),
          const Divider(height: 8, color: LokaTheme.border),

          // Related triples list
          Expanded(
            child: ListView(
              padding: const EdgeInsets.symmetric(horizontal: 12),
              children: relatedTriples.map((t) {
                return Padding(
                  padding: const EdgeInsets.symmetric(vertical: 3),
                  child: Text.rich(
                    TextSpan(children: [
                      TextSpan(
                        text: '${Triple.shortName(t.predicate)} ',
                        style: const TextStyle(
                            color: LokaTheme.green, fontSize: 11),
                      ),
                      TextSpan(
                        text: t.subject == node!.id
                            ? Triple.shortName(t.object)
                            : Triple.shortName(t.subject),
                        style: const TextStyle(
                            color: LokaTheme.text, fontSize: 11),
                      ),
                    ]),
                  ),
                );
              }).toList(),
            ),
          ),
        ],
      ),
    );
  }
}
