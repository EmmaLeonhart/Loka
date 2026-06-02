"""Tests for client-side OWL validation."""

import unittest
from loka.owl import OWLValidator, OWLViolation, OWL_DISJOINT


class _FakeClient:
    """Minimal client whose sparql() answers load_from_client's axiom queries.

    Returns a disjoint axiom (Cat disjointWith Dog) for the disjoint query and
    empty bindings for every other query.
    """

    def sparql(self, query: str) -> dict:
        if OWL_DISJOINT in query:
            return {
                "results": {
                    "bindings": [
                        {
                            "a": {"type": "uri", "value": "http://ex.org/Cat"},
                            "b": {"type": "uri", "value": "http://ex.org/Dog"},
                        }
                    ]
                }
            }
        return {"results": {"bindings": []}}


class TestOWLValidator(unittest.TestCase):
    def setUp(self):
        self.v = OWLValidator()

    def test_empty_validator_has_no_constraints(self):
        self.assertFalse(self.v.has_constraints())

    def test_domain_violation(self):
        self.v.domains["http://ex.org/worksAt"] = "http://ex.org/Person"
        self.v.entity_types["http://ex.org/car1"] = {"http://ex.org/Car"}
        result = self.v.validate_triple(
            "http://ex.org/car1",
            "http://ex.org/worksAt",
            "http://ex.org/company1",
        )
        self.assertIsNotNone(result)
        self.assertEqual(result.constraint_type, "domain")

    def test_domain_valid(self):
        self.v.domains["http://ex.org/worksAt"] = "http://ex.org/Person"
        self.v.entity_types["http://ex.org/alice"] = {"http://ex.org/Person"}
        result = self.v.validate_triple(
            "http://ex.org/alice",
            "http://ex.org/worksAt",
            "http://ex.org/company1",
        )
        self.assertIsNone(result)

    def test_range_violation(self):
        self.v.ranges["http://ex.org/knows"] = "http://ex.org/Person"
        self.v.entity_types["http://ex.org/car1"] = {"http://ex.org/Car"}
        result = self.v.validate_triple(
            "http://ex.org/alice",
            "http://ex.org/knows",
            "http://ex.org/car1",
        )
        self.assertIsNotNone(result)
        self.assertEqual(result.constraint_type, "range")

    def test_disjoint_violation(self):
        self.v.disjoint["http://ex.org/Cat"] = {"http://ex.org/Dog"}
        self.v.entity_types["http://ex.org/pet1"] = {"http://ex.org/Cat"}
        result = self.v.validate_triple(
            "http://ex.org/pet1",
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
            "http://ex.org/Dog",
        )
        self.assertIsNotNone(result)
        self.assertEqual(result.constraint_type, "disjoint")

    def test_subclass_hierarchy(self):
        self.v.subclass_of["http://ex.org/Student"] = {"http://ex.org/Person"}
        self.v.subclass_of["http://ex.org/Person"] = {"http://ex.org/Agent"}
        types = self.v.get_all_types("http://ex.org/Student")
        self.assertIn("http://ex.org/Student", types)
        self.assertIn("http://ex.org/Person", types)
        self.assertIn("http://ex.org/Agent", types)

    def test_generate_verification_queries(self):
        self.v.domains["http://ex.org/p"] = "http://ex.org/C"
        self.v.functional.add("http://ex.org/f")
        queries = self.v.generate_verification_queries()
        self.assertTrue(len(queries) >= 2)
        # Check queries are valid strings
        for desc, sparql in queries:
            self.assertIn("SELECT", sparql)
            self.assertTrue(len(desc) > 0)

    def test_validate_ntriples_no_violations(self):
        # No constraints loaded — everything passes
        violations = self.v.validate_ntriples(
            '<http://ex.org/a> <http://ex.org/b> <http://ex.org/c> .'
        )
        self.assertEqual(len(violations), 0)

    def test_owl_violation_is_exception(self):
        v = OWLViolation("test", "domain", ("s", "p", "o"))
        self.assertIsInstance(v, Exception)
        self.assertEqual(v.constraint_type, "domain")
        self.assertEqual(v.triple, ("s", "p", "o"))

    def test_load_from_client_loads_disjoint(self):
        # Regression: load_from_client previously never populated self.disjoint,
        # leaving the disjoint check in validate_triple dead. It must now load
        # owl:disjointWith symmetrically (matching the Java SDK port).
        self.v.load_from_client(_FakeClient())
        self.assertTrue(self.v.is_loaded())
        self.assertIn("http://ex.org/Dog", self.v.disjoint.get("http://ex.org/Cat", set()))
        self.assertIn("http://ex.org/Cat", self.v.disjoint.get("http://ex.org/Dog", set()))


if __name__ == "__main__":
    unittest.main()
