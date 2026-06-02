package io.github.emmaleonhart.loka;

import com.sun.net.httpserver.HttpExchange;
import com.sun.net.httpserver.HttpServer;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

import java.io.IOException;
import java.io.OutputStream;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.Set;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Unit tests for {@link OWLValidator}, mirroring the Python SDK's
 * {@code tests/test_owl.py}, plus a {@code loadFromClient} test using the
 * embedded JDK HTTP server.
 */
class OWLValidatorTest {

    private OWLValidator v;

    @BeforeEach
    void setUp() {
        v = new OWLValidator();
    }

    @Test
    void emptyValidatorHasNoConstraints() {
        assertFalse(v.hasConstraints());
        assertFalse(v.isLoaded());
    }

    @Test
    void domainViolation() {
        v.domains().put("http://ex.org/worksAt", "http://ex.org/Person");
        v.entityTypes().put("http://ex.org/car1", Set.of("http://ex.org/Car"));
        OWLViolation result = v.validateTriple(
                "http://ex.org/car1", "http://ex.org/worksAt", "http://ex.org/company1");
        assertNotNull(result);
        assertEquals("domain", result.getConstraintType());
    }

    @Test
    void domainValid() {
        v.domains().put("http://ex.org/worksAt", "http://ex.org/Person");
        v.entityTypes().put("http://ex.org/alice", Set.of("http://ex.org/Person"));
        OWLViolation result = v.validateTriple(
                "http://ex.org/alice", "http://ex.org/worksAt", "http://ex.org/company1");
        assertNull(result);
    }

    @Test
    void rangeViolation() {
        v.ranges().put("http://ex.org/knows", "http://ex.org/Person");
        v.entityTypes().put("http://ex.org/car1", Set.of("http://ex.org/Car"));
        OWLViolation result = v.validateTriple(
                "http://ex.org/alice", "http://ex.org/knows", "http://ex.org/car1");
        assertNotNull(result);
        assertEquals("range", result.getConstraintType());
    }

    @Test
    void rangeIgnoresLiteralObjects() {
        v.ranges().put("http://ex.org/knows", "http://ex.org/Person");
        // A literal object (starts with a quote) is never a range violation.
        OWLViolation result = v.validateTriple(
                "http://ex.org/alice", "http://ex.org/knows", "\"not an IRI\"");
        assertNull(result);
    }

    @Test
    void disjointViolation() {
        v.disjoint().put("http://ex.org/Cat", Set.of("http://ex.org/Dog"));
        v.entityTypes().put("http://ex.org/pet1", Set.of("http://ex.org/Cat"));
        OWLViolation result = v.validateTriple(
                "http://ex.org/pet1", OWLValidator.RDF_TYPE, "http://ex.org/Dog");
        assertNotNull(result);
        assertEquals("disjoint", result.getConstraintType());
    }

    @Test
    void subclassHierarchyIsTransitive() {
        v.subclassOf().put("http://ex.org/Student", Set.of("http://ex.org/Person"));
        v.subclassOf().put("http://ex.org/Person", Set.of("http://ex.org/Agent"));
        Set<String> types = v.getAllTypes("http://ex.org/Student");
        assertTrue(types.contains("http://ex.org/Student"));
        assertTrue(types.contains("http://ex.org/Person"));
        assertTrue(types.contains("http://ex.org/Agent"));
    }

    @Test
    void domainSatisfiedViaSubclass() {
        // worksAt requires Person; alice is a Student which is a subclass of Person.
        v.domains().put("http://ex.org/worksAt", "http://ex.org/Person");
        v.subclassOf().put("http://ex.org/Student", Set.of("http://ex.org/Person"));
        v.entityTypes().put("http://ex.org/alice", Set.of("http://ex.org/Student"));
        assertNull(v.validateTriple(
                "http://ex.org/alice", "http://ex.org/worksAt", "http://ex.org/company1"));
    }

    @Test
    void generateVerificationQueries() {
        v.domains().put("http://ex.org/p", "http://ex.org/C");
        v.functional().add("http://ex.org/f");
        List<OWLValidator.VerificationQuery> queries = v.generateVerificationQueries();
        assertTrue(queries.size() >= 2);
        for (OWLValidator.VerificationQuery q : queries) {
            assertTrue(q.getSparql().contains("SELECT"));
            assertFalse(q.getDescription().isEmpty());
        }
    }

    @Test
    void validateNtriplesNoViolationsWhenUnconstrained() {
        List<OWLViolation> violations = v.validateNtriples(
                "<http://ex.org/a> <http://ex.org/b> <http://ex.org/c> .");
        assertEquals(0, violations.size());
    }

    @Test
    void validateNtriplesFindsDomainViolation() {
        v.domains().put("http://ex.org/worksAt", "http://ex.org/Person");
        v.entityTypes().put("http://ex.org/car1", Set.of("http://ex.org/Car"));
        List<OWLViolation> violations = v.validateNtriples(
                "# a comment\n"
                        + "<http://ex.org/car1> <http://ex.org/worksAt> <http://ex.org/company1> .\n");
        assertEquals(1, violations.size());
        assertEquals("domain", violations.get(0).getConstraintType());
    }

    @Test
    void owlViolationCarriesConstraintTypeAndTriple() {
        OWLViolation violation = new OWLViolation("test", "domain", "s", "p", "o");
        assertTrue(violation instanceof RuntimeException);
        assertEquals("domain", violation.getConstraintType());
        assertEquals(List.of("s", "p", "o"), violation.getTriple());
    }

    // ---- loadFromClient (embedded server) ----

    @Test
    void loadFromClientParsesAxioms() throws IOException {
        HttpServer server = HttpServer.create(new InetSocketAddress(0), 0);
        server.setExecutor(null);
        server.createContext("/sparql", exchange -> {
            String body = new String(exchange.getRequestBody().readAllBytes(), StandardCharsets.UTF_8);
            if (body.contains(OWLValidator.OWL_FUNCTIONAL)) {
                respond(exchange, 200, bindings("[\"p\"]", "")); // no functional props
            } else if (body.contains(OWLValidator.RDFS_DOMAIN)) {
                respond(exchange, 200, bindings("[\"p\",\"d\"]",
                        binding("p", "http://ex.org/worksAt") + "," + bindingTail("d", "http://ex.org/Person")));
            } else if (body.contains(OWLValidator.OWL_DISJOINT)) {
                respond(exchange, 200, bindings("[\"a\",\"b\"]",
                        binding("a", "http://ex.org/Cat") + "," + bindingTail("b", "http://ex.org/Dog")));
            } else if (body.contains(OWLValidator.RDF_TYPE)) {
                // entity-types query (rdf:type, not the functional one handled above)
                respond(exchange, 200, bindings("[\"e\",\"t\"]",
                        binding("e", "http://ex.org/alice") + "," + bindingTail("t", "http://ex.org/Person")));
            } else {
                respond(exchange, 200, bindings("[]", "")); // empty for the rest
            }
        });
        server.start();
        try {
            int port = server.getAddress().getPort();
            OWLValidator validator = new OWLValidator();
            validator.loadFromClient(new LokaClient("http://localhost:" + port));

            assertTrue(validator.isLoaded());
            assertEquals("http://ex.org/Person", validator.domains().get("http://ex.org/worksAt"));
            // disjoint is loaded symmetrically (the Java port adds the query the Python SDK omits)
            assertTrue(validator.disjoint().get("http://ex.org/Cat").contains("http://ex.org/Dog"));
            assertTrue(validator.disjoint().get("http://ex.org/Dog").contains("http://ex.org/Cat"));
            assertTrue(validator.entityTypes().get("http://ex.org/alice").contains("http://ex.org/Person"));
        } finally {
            server.stop(0);
        }
    }

    // ---- helpers ----

    private static String binding(String var, String value) {
        return "\"" + var + "\":{\"type\":\"uri\",\"value\":\"" + value + "\"}";
    }

    private static String bindingTail(String var, String value) {
        return binding(var, value);
    }

    private static String bindings(String varsJson, String rowInner) {
        String rows = rowInner.isEmpty() ? "" : "{" + rowInner + "}";
        return "{\"head\":{\"vars\":" + varsJson + "},\"results\":{\"bindings\":[" + rows + "]}}";
    }

    private static void respond(HttpExchange exchange, int status, String body) throws IOException {
        byte[] bytes = body.getBytes(StandardCharsets.UTF_8);
        exchange.getResponseHeaders().set("Content-Type", "application/json");
        exchange.sendResponseHeaders(status, bytes.length);
        try (OutputStream os = exchange.getResponseBody()) {
            os.write(bytes);
        }
    }
}
