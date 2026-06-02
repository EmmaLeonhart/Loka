package io.github.emmaleonhart.loka;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Deque;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * Client-side OWL constraint validator.
 *
 * <p>Loka accepts all triples unconditionally; OWL validation happens here in
 * the SDK before sending data to the server ("lean store, smart client"). This
 * is a faithful port of the Python SDK's {@code loka.owl} module. It loads
 * ontology axioms from a Loka instance, caches them, and checks triples
 * against:</p>
 *
 * <ul>
 *   <li>{@code rdfs:domain} — property subject-type constraints</li>
 *   <li>{@code rdfs:range} — property object-type constraints</li>
 *   <li>{@code rdfs:subClassOf} — type hierarchy (used to satisfy domain/range)</li>
 *   <li>{@code owl:disjointWith} — classes that cannot overlap</li>
 *   <li>{@code owl:FunctionalProperty} — at most one value (verification query)</li>
 * </ul>
 *
 * <p>The internal axiom collections are exposed via accessor methods that
 * return the live, mutable maps, mirroring the Python module's public
 * attributes — convenient for tests and for callers that populate axioms
 * directly.</p>
 */
public class OWLValidator {

    /** {@code rdf:type}. */
    public static final String RDF_TYPE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    /** {@code rdfs:domain}. */
    public static final String RDFS_DOMAIN = "http://www.w3.org/2000/01/rdf-schema#domain";
    /** {@code rdfs:range}. */
    public static final String RDFS_RANGE = "http://www.w3.org/2000/01/rdf-schema#range";
    /** {@code rdfs:subClassOf}. */
    public static final String RDFS_SUBCLASS_OF = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
    /** {@code owl:FunctionalProperty}. */
    public static final String OWL_FUNCTIONAL = "http://www.w3.org/2002/07/owl#FunctionalProperty";
    /** {@code owl:disjointWith}. */
    public static final String OWL_DISJOINT = "http://www.w3.org/2002/07/owl#disjointWith";
    /** {@code owl:equivalentClass}. */
    public static final String OWL_EQUIVALENT_CLASS = "http://www.w3.org/2002/07/owl#equivalentClass";
    /** {@code owl:sameAs}. */
    public static final String OWL_SAME_AS = "http://www.w3.org/2002/07/owl#sameAs";
    /** {@code owl:inverseOf}. */
    public static final String OWL_INVERSE_OF = "http://www.w3.org/2002/07/owl#inverseOf";
    /** {@code rdfs:subPropertyOf}. */
    public static final String RDFS_SUB_PROPERTY_OF = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf";

    private final Map<String, String> domains = new HashMap<>();
    private final Map<String, String> ranges = new HashMap<>();
    private final Map<String, Set<String>> subclassOf = new HashMap<>();
    private final Map<String, Set<String>> subPropertyOf = new HashMap<>();
    private final Set<String> functional = new HashSet<>();
    private final Map<String, Set<String>> disjoint = new HashMap<>();
    private final Map<String, Set<String>> equivalentClasses = new HashMap<>();
    private final Map<String, Set<String>> sameAs = new HashMap<>();
    private final Map<String, String> inverseOf = new HashMap<>();
    private final Map<String, Set<String>> entityTypes = new HashMap<>();
    private boolean loaded = false;

    // ---- accessors for the live axiom collections (mirror Python attributes) ----

    /** Property &rarr; domain class. */
    public Map<String, String> domains() { return domains; }
    /** Property &rarr; range class. */
    public Map<String, String> ranges() { return ranges; }
    /** Class &rarr; direct parent classes. */
    public Map<String, Set<String>> subclassOf() { return subclassOf; }
    /** Property &rarr; parent properties. */
    public Map<String, Set<String>> subPropertyOf() { return subPropertyOf; }
    /** Functional properties. */
    public Set<String> functional() { return functional; }
    /** Class &rarr; disjoint classes. */
    public Map<String, Set<String>> disjoint() { return disjoint; }
    /** Class &rarr; equivalent classes. */
    public Map<String, Set<String>> equivalentClasses() { return equivalentClasses; }
    /** Entity &rarr; same-as entities. */
    public Map<String, Set<String>> sameAs() { return sameAs; }
    /** Property &rarr; inverse property. */
    public Map<String, String> inverseOf() { return inverseOf; }
    /** Entity &rarr; declared types. */
    public Map<String, Set<String>> entityTypes() { return entityTypes; }

    /**
     * Load OWL ontology axioms from a Loka client.
     *
     * @param client a connected {@link LokaClient}
     */
    public void loadFromClient(LokaClient client) {
        for (Map<String, SparqlResults.BindingValue> row :
                client.sparql("SELECT ?p ?d WHERE { ?p <" + RDFS_DOMAIN + "> ?d }").getBindings()) {
            String p = value(row, "p");
            String d = value(row, "d");
            if (!p.isEmpty() && !d.isEmpty()) {
                domains.put(p, d);
            }
        }

        for (Map<String, SparqlResults.BindingValue> row :
                client.sparql("SELECT ?p ?r WHERE { ?p <" + RDFS_RANGE + "> ?r }").getBindings()) {
            String p = value(row, "p");
            String r = value(row, "r");
            if (!p.isEmpty() && !r.isEmpty()) {
                ranges.put(p, r);
            }
        }

        for (Map<String, SparqlResults.BindingValue> row :
                client.sparql("SELECT ?c ?parent WHERE { ?c <" + RDFS_SUBCLASS_OF + "> ?parent }").getBindings()) {
            String c = value(row, "c");
            String parent = value(row, "parent");
            if (!c.isEmpty() && !parent.isEmpty()) {
                subclassOf.computeIfAbsent(c, k -> new HashSet<>()).add(parent);
            }
        }

        for (Map<String, SparqlResults.BindingValue> row :
                client.sparql("SELECT ?p WHERE { ?p <" + RDF_TYPE + "> <" + OWL_FUNCTIONAL + "> }").getBindings()) {
            String p = value(row, "p");
            if (!p.isEmpty()) {
                functional.add(p);
            }
        }

        for (Map<String, SparqlResults.BindingValue> row :
                client.sparql("SELECT ?p ?parent WHERE { ?p <" + RDFS_SUB_PROPERTY_OF + "> ?parent }").getBindings()) {
            String p = value(row, "p");
            String parent = value(row, "parent");
            if (!p.isEmpty() && !parent.isEmpty()) {
                subPropertyOf.computeIfAbsent(p, k -> new HashSet<>()).add(parent);
            }
        }

        // Disjoint classes (symmetric). NOTE: the Python SDK's load_from_client
        // omits this query, leaving its disjoint check dead; the Java port loads
        // it so disjoint validation actually fires. See queue.md for the Python
        // follow-up to reconverge.
        for (Map<String, SparqlResults.BindingValue> row :
                client.sparql("SELECT ?a ?b WHERE { ?a <" + OWL_DISJOINT + "> ?b }").getBindings()) {
            String a = value(row, "a");
            String b = value(row, "b");
            if (!a.isEmpty() && !b.isEmpty()) {
                disjoint.computeIfAbsent(a, k -> new HashSet<>()).add(b);
                disjoint.computeIfAbsent(b, k -> new HashSet<>()).add(a);
            }
        }

        for (Map<String, SparqlResults.BindingValue> row :
                client.sparql("SELECT ?a ?b WHERE { ?a <" + OWL_EQUIVALENT_CLASS + "> ?b }").getBindings()) {
            String a = value(row, "a");
            String b = value(row, "b");
            if (!a.isEmpty() && !b.isEmpty()) {
                equivalentClasses.computeIfAbsent(a, k -> new HashSet<>()).add(b);
                equivalentClasses.computeIfAbsent(b, k -> new HashSet<>()).add(a);
            }
        }

        for (Map<String, SparqlResults.BindingValue> row :
                client.sparql("SELECT ?a ?b WHERE { ?a <" + OWL_SAME_AS + "> ?b }").getBindings()) {
            String a = value(row, "a");
            String b = value(row, "b");
            if (!a.isEmpty() && !b.isEmpty()) {
                sameAs.computeIfAbsent(a, k -> new HashSet<>()).add(b);
                sameAs.computeIfAbsent(b, k -> new HashSet<>()).add(a);
            }
        }

        for (Map<String, SparqlResults.BindingValue> row :
                client.sparql("SELECT ?p ?inv WHERE { ?p <" + OWL_INVERSE_OF + "> ?inv }").getBindings()) {
            String p = value(row, "p");
            String inv = value(row, "inv");
            if (!p.isEmpty() && !inv.isEmpty()) {
                inverseOf.put(p, inv);
                inverseOf.put(inv, p);
            }
        }

        for (Map<String, SparqlResults.BindingValue> row :
                client.sparql("SELECT ?e ?t WHERE { ?e <" + RDF_TYPE + "> ?t } LIMIT 10000").getBindings()) {
            String e = value(row, "e");
            String t = value(row, "t");
            if (!e.isEmpty() && !t.isEmpty()) {
                entityTypes.computeIfAbsent(e, k -> new HashSet<>()).add(t);
            }
        }

        loaded = true;
    }

    /** Whether {@link #loadFromClient} has completed. */
    public boolean isLoaded() {
        return loaded;
    }

    /** Whether any constraints exist that {@link #validateTriple} can act on. */
    public boolean hasConstraints() {
        return !domains.isEmpty() || !ranges.isEmpty() || !functional.isEmpty() || !disjoint.isEmpty();
    }

    /**
     * Return a class and all of its ancestors via {@code rdfs:subClassOf}.
     *
     * @param classIri the class IRI
     * @return the class plus all transitive parents
     */
    public Set<String> getAllTypes(String classIri) {
        Set<String> result = new HashSet<>();
        result.add(classIri);
        Deque<String> queue = new ArrayDeque<>();
        queue.push(classIri);
        while (!queue.isEmpty()) {
            String current = queue.pop();
            for (String parent : subclassOf.getOrDefault(current, java.util.Collections.emptySet())) {
                if (result.add(parent)) {
                    queue.push(parent);
                }
            }
        }
        return result;
    }

    /**
     * Validate a single triple against the loaded OWL constraints.
     *
     * @param subject   the triple subject
     * @param predicate the triple predicate
     * @param object    the triple object
     * @return an {@link OWLViolation} if the triple is invalid, or {@code null} if valid
     */
    public OWLViolation validateTriple(String subject, String predicate, String object) {
        // Domain check
        if (domains.containsKey(predicate)) {
            String expectedDomain = domains.get(predicate);
            Set<String> subjectTypes = entityTypes.get(subject);
            if (subjectTypes != null && !subjectTypes.isEmpty()) {
                Set<String> allTypes = new HashSet<>();
                for (String t : subjectTypes) {
                    allTypes.addAll(getAllTypes(t));
                }
                if (!allTypes.contains(expectedDomain)) {
                    return new OWLViolation(
                            "Domain violation: " + predicate + " requires subject of type "
                                    + expectedDomain + ", but " + subject + " has types " + subjectTypes,
                            "domain", subject, predicate, object);
                }
            }
        }

        // Range check (skip literal objects, which start with a quote)
        if (ranges.containsKey(predicate) && !object.startsWith("\"")) {
            String expectedRange = ranges.get(predicate);
            Set<String> objectTypes = entityTypes.get(object);
            if (objectTypes != null && !objectTypes.isEmpty()) {
                Set<String> allTypes = new HashSet<>();
                for (String t : objectTypes) {
                    allTypes.addAll(getAllTypes(t));
                }
                if (!allTypes.contains(expectedRange)) {
                    return new OWLViolation(
                            "Range violation: " + predicate + " requires object of type "
                                    + expectedRange + ", but " + object + " has types " + objectTypes,
                            "range", subject, predicate, object);
                }
            }
        }

        // Disjoint class check (when assigning a type)
        if (RDF_TYPE.equals(predicate)) {
            Set<String> existingTypes = entityTypes.getOrDefault(subject, java.util.Collections.emptySet());
            for (String existingType : existingTypes) {
                Set<String> disjointSet = disjoint.get(existingType);
                if (disjointSet != null && disjointSet.contains(object)) {
                    return new OWLViolation(
                            "Disjoint violation: " + subject + " is already type " + existingType
                                    + ", which is disjoint with " + object,
                            "disjoint", subject, predicate, object);
                }
            }
        }

        return null; // valid
    }

    /**
     * Generate SPARQL queries that each return rows representing OWL constraint
     * violations present in the database.
     *
     * @return a list of (description, SPARQL) pairs
     */
    public List<VerificationQuery> generateVerificationQueries() {
        List<VerificationQuery> queries = new ArrayList<>();

        for (Map.Entry<String, String> e : domains.entrySet()) {
            String prop = e.getKey();
            String domainClass = e.getValue();
            queries.add(new VerificationQuery(
                    "Domain violation: " + prop + " requires subject of type " + domainClass,
                    "SELECT ?s WHERE { ?s <" + prop + "> ?o . "
                            + "FILTER NOT EXISTS { ?s <" + RDF_TYPE + "> <" + domainClass + "> } }"));
        }

        for (Map.Entry<String, String> e : ranges.entrySet()) {
            String prop = e.getKey();
            String rangeClass = e.getValue();
            queries.add(new VerificationQuery(
                    "Range violation: " + prop + " requires object of type " + rangeClass,
                    "SELECT ?o WHERE { ?s <" + prop + "> ?o . "
                            + "FILTER NOT EXISTS { ?o <" + RDF_TYPE + "> <" + rangeClass + "> } }"));
        }

        for (String prop : functional) {
            queries.add(new VerificationQuery(
                    "Functional violation: " + prop + " should have at most one value per subject",
                    "SELECT ?s WHERE { ?s <" + prop + "> ?o1 . ?s <" + prop + "> ?o2 . "
                            + "FILTER(?o1 != ?o2) }"));
        }

        for (Map.Entry<String, Set<String>> e : disjoint.entrySet()) {
            String cls = e.getKey();
            for (String other : e.getValue()) {
                queries.add(new VerificationQuery(
                        "Disjoint violation: " + cls + " and " + other + " cannot overlap",
                        "SELECT ?x WHERE { ?x <" + RDF_TYPE + "> <" + cls + "> . "
                                + "?x <" + RDF_TYPE + "> <" + other + "> }"));
            }
        }

        return queries;
    }

    /**
     * Validate a block of N-Triples, returning every violation found.
     *
     * @param ntriples N-Triples text (one triple per line)
     * @return the list of violations (empty if all valid)
     */
    public List<OWLViolation> validateNtriples(String ntriples) {
        List<OWLViolation> violations = new ArrayList<>();
        for (String rawLine : ntriples.split("\n", -1)) {
            String line = rawLine.trim();
            if (line.isEmpty() || line.startsWith("#")) {
                continue;
            }
            String[] parts = line.split("\\s+", 3);
            if (parts.length < 3) {
                continue;
            }
            String s = stripChars(parts[0], "<>");
            String p = stripChars(parts[1], "<>");
            String oRaw = stripTrailing(parts[2], " .");
            String o = oRaw.startsWith("<") ? stripChars(oRaw, "<>") : oRaw;

            OWLViolation violation = validateTriple(s, p, o);
            if (violation != null) {
                violations.add(violation);
            }
        }
        return violations;
    }

    private static String value(Map<String, SparqlResults.BindingValue> row, String var) {
        SparqlResults.BindingValue bv = row.get(var);
        return bv == null ? "" : bv.getValue();
    }

    /** Strip any leading/trailing characters that appear in {@code chars} (Python str.strip semantics). */
    private static String stripChars(String s, String chars) {
        int start = 0;
        int end = s.length();
        while (start < end && chars.indexOf(s.charAt(start)) >= 0) {
            start++;
        }
        while (end > start && chars.indexOf(s.charAt(end - 1)) >= 0) {
            end--;
        }
        return s.substring(start, end);
    }

    /** Strip any trailing characters that appear in {@code chars} (Python str.rstrip semantics). */
    private static String stripTrailing(String s, String chars) {
        int end = s.length();
        while (end > 0 && chars.indexOf(s.charAt(end - 1)) >= 0) {
            end--;
        }
        return s.substring(0, end);
    }

    /** A (description, SPARQL) pair describing a constraint-violation query. */
    public static final class VerificationQuery {
        private final String description;
        private final String sparql;

        VerificationQuery(String description, String sparql) {
            this.description = description;
            this.sparql = sparql;
        }

        /** Human-readable description of what the query checks. */
        public String getDescription() {
            return description;
        }

        /** The SPARQL query that returns violation rows. */
        public String getSparql() {
            return sparql;
        }
    }
}
