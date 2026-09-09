"""Explicit scaffold and wrapper-generation commands for application wheels."""

import argparse
import json
from pathlib import Path

from . import CodegenError, emit, scaffold


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    create = commands.add_parser(
        "scaffold", help="write a new native application module project"
    )
    create.add_argument("config", type=Path)
    create.add_argument("--output", type=Path, required=True)
    create.add_argument("--pgorm-source", type=Path, required=True)
    generate = commands.add_parser(
        "emit", help="emit concrete wrappers from the installed application registry"
    )
    generate.add_argument("project", type=Path)
    options = parser.parse_args()
    try:
        if options.command == "scaffold":
            result = scaffold(
                json.loads(options.config.read_text()),
                options.output,
                pgorm_source=options.pgorm_source,
                base=options.config.resolve().parent,
            )
        else:
            result = emit(options.project)
    except (CodegenError, OSError, json.JSONDecodeError) as error:
        parser.error(str(error))
    print(result)


if __name__ == "__main__":
    main()
