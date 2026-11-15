"""Entry point for the packaged desktop app: opens the window unless given a command."""

import sys

from pocket_dm.cli import build_parser, main

COMMANDS = {a for action in build_parser()._subparsers._group_actions for a in action.choices}

if __name__ == "__main__":
    args = sys.argv[1:]
    if not COMMANDS.intersection(args) and not {"-h", "--help", "--version"}.intersection(args):
        args.append("app")  # e.g. `"Pocket DM" --data-dir D:\table` still opens the window
    raise SystemExit(main(args))
