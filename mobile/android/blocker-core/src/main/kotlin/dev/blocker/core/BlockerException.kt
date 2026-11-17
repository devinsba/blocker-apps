package dev.blocker.core

/** A failure reported by the Rust core (the text of `blocker_last_error()`). */
class BlockerException @JvmOverloads constructor(
    message: String,
    cause: Throwable? = null,
) : RuntimeException(message, cause)
