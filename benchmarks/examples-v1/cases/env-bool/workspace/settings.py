"""Read a boolean setting from an environment-style string."""


def parse_bool(value, default=False):
    if value is None:
        return default
    return bool(value)
