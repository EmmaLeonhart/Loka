package io.github.emmaleonhart.loka;

import java.util.Arrays;
import java.util.Collections;
import java.util.List;

/**
 * Thrown (or returned) when a triple violates an OWL constraint.
 *
 * <p>Mirrors the Python SDK's {@code loka.owl.OWLViolation}. Carries the
 * constraint type that was violated ({@code "domain"}, {@code "range"},
 * {@code "disjoint"}, ...) and the offending triple as
 * {@code [subject, predicate, object]}.</p>
 */
public class OWLViolation extends RuntimeException {

    private final String constraintType;
    private final List<String> triple;

    /**
     * Create a new violation.
     *
     * @param message        human-readable description of the violation
     * @param constraintType the kind of constraint violated (e.g. {@code "domain"})
     * @param subject        the offending triple's subject
     * @param predicate      the offending triple's predicate
     * @param object         the offending triple's object
     */
    public OWLViolation(String message, String constraintType,
                        String subject, String predicate, String object) {
        super(message);
        this.constraintType = constraintType;
        this.triple = Collections.unmodifiableList(Arrays.asList(subject, predicate, object));
    }

    /** Return the kind of constraint that was violated. */
    public String getConstraintType() {
        return constraintType;
    }

    /** Return the offending triple as {@code [subject, predicate, object]}. */
    public List<String> getTriple() {
        return triple;
    }
}
