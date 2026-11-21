package dev.blocker.core;

/** Host twin of the Kotlin BlockerException (the shim needs a (String) constructor). */
public class BlockerException extends RuntimeException {
    public BlockerException(String message) {
        super(message);
    }
}
