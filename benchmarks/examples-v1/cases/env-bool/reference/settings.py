"""Read a boolean setting from an environment-style string."""


def parse_bool(value, default=False):
    if value is None or not value.strip():
        return default
    normalized = value.strip().lower()
    if normalized in ("true", "1", "yes", "on"):
        return True
    if normalized in ("false", "0", "no", "off"):
        return False
    raise ValueError(f"Invalid boolean setting: {value!r}")
