#!/usr/bin/env python3
from asyncio import create_subprocess_exec
from subprocess import DEVNULL


async def remote(address, command):
    p = await create_subprocess_exec('ssh', '-q', address, command, stdout=DEVNULL)
    await p.wait()
    assert p.returncode == 0


async def setup_remote(address, spec):
    await remote(
        address,
        f"for i in $(seq {spec['#core'] // 2} {spec['#core'] - 1}); do "
            "echo 0 | sudo tee /sys/devices/system/cpu/cpu$i/online; done && "
        f"sudo ethtool -L {spec['interface']} combined 1 && "
        f"sudo ethtool -G {spec['interface']} rx {spec['rx']} tx {spec['tx']} && "  #
        "sudo service irqbalance stop && "
        f"IRQBALANCE_BANNED_CPULIST=0-{spec['#core'] // 2 - 2} sudo -E irqbalance --oneshot && "
        # for working with AWS VPC's multicast
        f"sudo sysctl net.ipv4.conf.{spec['interface']}.force_igmp_version=2")


if __name__ == '__main__':
    from asyncio import run, gather
    from sys import argv
    from runpy import run_path

    specs = run_path(argv[1])
    roles = argv[2:]
    assert all(f'spec_{role}' in specs for role in roles)

    tasks = []
    with open('run_addr.txt') as addresses:
        for line in addresses:
            [role, address, _] = line.split()
            if role in roles:
                tasks.append(setup_remote(address, specs[f'spec_{role}']))

    async def main():
        await gather(*tasks)

    run(main())
