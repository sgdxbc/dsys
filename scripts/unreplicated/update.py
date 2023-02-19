#!/usr/bin/env python3
import sys
import asyncio

sys.path.append("scripts")
from lib import build, rsync, load_instances


async def main():
    await build("dsys")
    await asyncio.gather(
        *(
            instance.rsync("target/release/unreplicated")
            for instance in load_instances()
            if instance.role in {"replica", "client"}
        )
    )


if __name__ == "__main__":
    asyncio.run(main())
else:
    print("execute this script")
    exit(1)
