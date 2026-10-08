package io.github.emmaleonhart.loka;

import org.json.JSONObject;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.condition.EnabledIfEnvironmentVariable;

import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Round trip against a real Loka server: insert triples, query them back,
 * check the values. Runs only when LOKA_ENDPOINT is set (CI starts a server
 * for it); the unit tests in LokaClientTest use a mock server instead.
 */
@EnabledIfEnvironmentVariable(named = "LOKA_ENDPOINT", matches = ".+")
class LokaIntegrationTest {

    private static final String EX = "http://example.org/java-it/";

    private LokaClient client() {
        return new LokaClient(System.getenv("LOKA_ENDPOINT"));
    }

    @Test
    void healthIsUp() {
        assertTrue(client().health());
    }

    @Test
    void insertThenQueryRoundTripsIrisAndLiterals() {
        LokaClient c = client();
        String nt = String.join("\n",
                "<" + EX + "ada> <" + EX + "name> \"Ada Lovelace\" .",
                "<" + EX + "ada> <" + EX + "knows> <" + EX + "charles> .",
                "<" + EX + "charles> <" + EX + "name> \"Charles Babbage\" .",
                "<" + EX + "zoe> <" + EX + "name> \"Zoë\" .");
        JSONObject ins = c.insertTriples(nt);
        assertEquals(4, ins.getInt("inserted"), ins.toString());

        SparqlResults r = c.sparql(
                "SELECT ?n WHERE { <" + EX + "ada> <" + EX + "knows> ?f . ?f <" + EX + "name> ?n }");
        assertEquals(1, r.size());
        assertEquals("Charles Babbage", r.getBindings().get(0).get("n").getValue());

        // Non-ASCII survives the round trip.
        SparqlResults z = c.sparql("SELECT ?n WHERE { <" + EX + "zoe> <" + EX + "name> ?n }");
        assertEquals("Zoë", z.getBindings().get(0).get("n").getValue());
    }

    @Test
    void rdfStarAnnotationRoundTrips() {
        LokaClient c = client();
        String nt = String.join("\n",
                "<" + EX + "s> <" + EX + "p> <" + EX + "o> .",
                "<< <" + EX + "s> <" + EX + "p> <" + EX + "o> >> <" + EX + "source> <" + EX + "census> .");
        c.insertTriples(nt);

        SparqlResults r = c.sparql(
                "SELECT ?s ?src WHERE { << ?s <" + EX + "p> <" + EX + "o> >> <" + EX + "source> ?src }");
        assertEquals(1, r.size());
        Map<String, SparqlResults.BindingValue> row = r.getBindings().get(0);
        assertEquals(EX + "s", row.get("s").getValue());
        assertEquals(EX + "census", row.get("src").getValue());
    }

    @Test
    void orderByReturnsValuesInOrder() {
        LokaClient c = client();
        // Inserted out of alphabetical order.
        c.insertTriples(String.join("\n",
                "<" + EX + "o3> <" + EX + "label> \"cherry\" .",
                "<" + EX + "o1> <" + EX + "label> \"apple\" .",
                "<" + EX + "o2> <" + EX + "label> \"banana\" ."));
        SparqlResults r = c.sparql(
                "SELECT ?l WHERE { ?x <" + EX + "label> ?l } ORDER BY ?l");
        List<Map<String, SparqlResults.BindingValue>> rows = r.getBindings();
        Map<Integer, String> got = new HashMap<>();
        for (int i = 0; i < rows.size(); i++) {
            got.put(i, rows.get(i).get("l").getValue());
        }
        assertEquals(Map.of(0, "apple", 1, "banana", 2, "cherry"), got);
    }
}
