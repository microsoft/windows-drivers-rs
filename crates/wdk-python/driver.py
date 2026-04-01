"""Sample Windows Driver in Python."""

STATUS_SUCCESS = 0


def driver_entry():
    """Entry point for the driver."""
    print("Loading driver...")
    return STATUS_SUCCESS


if __name__ == "__main__":
    import sys
    sys.exit(driver_entry())
