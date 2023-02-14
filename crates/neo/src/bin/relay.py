#!/usr/bin/env python3
from asyncio import create_subprocess_exec, gather, sleep

async def remote(address, args, stdout=None, stderr=None):
    return await create_subprocess_exec(
        'ssh', '-q', address, *args, stdout=stdout, stderr=stderr)

async def remote_sync(address, args):
    p = await remote(address, args)
    await p.wait()
    return p.returncode

if __name__ == '__main__':
    from sys import argv
    from asyncio import run, gather

    if argv[1] == 'setup':
        i = 1
        tasks = []
        with open('addresses.txt') as addresses:
            for line in addresses:
                [role, public_address, _] = line.split()
                if role == 'relay':
                    tasks.append(remote_sync(
                        public_address, [
                            'tmux', 'new-session', '-d', '-s', 'neo',
                            './neo-relay', f'239.255.2.{i}']))
                    i += 1
        async def main():
            await gather(*tasks)
        run(main())
        exit()
    if argv[1] == 'shutdown':
        tasks = []
        with open('addresses.txt') as addresses:
            for line in addresses:
                [role, public_address, _] = line.split()
                if role == 'relay':
                    tasks.append(remote_sync(public_address, ['tmux', 'send-key', '-t', 'neo', 'C-c']))
        async def main():
            await gather(*tasks)
        run(main())
        exit()

    print(f'{argv[0]} setup|shutdown')
