#!/usr/bin/env python3
from pyrem.host import RemoteHost
from pyrem.task import Parallel, Sequential
from sys import argv

try:
    setup_role, interface, n = argv[1], argv[2], int(argv[3])
except:
    print(f'Usage: {argv[0]} [role] [interface] [#core]')
    exit(1)
tasks = []
with open('addresses.txt') as addresses:
    for line in addresses:
        [role, address] = line.split()
        if role != setup_role:
            continue
        task = Sequential([
            RemoteHost(address).run(['sudo', 'ethtool', '-L', interface, 'combined', '1']),
            RemoteHost(address).run(['echo', f'IRQBALANCE_BANNED_CPULIST=0-{n - 2}', '|', 'sudo', 'tee', '/etc/default/irqbalance']),
            RemoteHost(address).run(['sudo', 'service', 'irqbalance', 'restart']),
            # EC2 specific multicast setup
            RemoteHost(address).run(['sudo', 'sysctl', f'net.ipv4.conf.{interface}.force_igmp_version=2'])])
        tasks.append(task)
Parallel(tasks).start(wait=True)
