"""Questodian: a campaign manager and DM screen for 5e tabletop games."""

__version__ = "0.1.1"


def main() -> None:
    from .cli import main as cli_main

    raise SystemExit(cli_main())
