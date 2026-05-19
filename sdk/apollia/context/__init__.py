"""ctx.* typed surfaces — Protocols consumed by the Rust runtime (ADR-101).

This package defines only contracts (``typing.Protocol``).  The actual
backend implementations live in ``apollia-aip`` (Rust → Python bridge)
and are bound at task startup.
"""
