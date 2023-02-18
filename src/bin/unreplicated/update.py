#!/usr/bin/env python3
from asyncio import create_subprocess_shell, gather, run


async def local(command, check_return=0):
    p = await create_subprocess_shell(command)
    await p.wait()
    return p.returncode


async def main():
    if await local('cargo build --release --bin unreplicated'):
        return

    tasks = []
    with open('run_addr.txt') as f:
        for line in f:
            [role, address, _] = line.split()
            if role in {'replica', 'client'}:
                tasks.append(local(
                    f'rsync --rsh="ssh -q" --update --times target/release/unreplicated {address}:'))
    await gather(*tasks)


if __name__ == '__main__':
    run(main())
