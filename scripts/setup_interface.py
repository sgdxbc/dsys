#!/usr/bin/env python3
from pyrem.host import RemoteHost
from pyrem.task import Parallel, Sequential
from sys import argv

interface = argv[1]
tasks = []
with open('addresses.txt') as addresses:
    for line in addresses:
        [role, address] = line.split()
        if role == 'replica':
            task = Sequential([
                RemoteHost(address).run(['sudo', 'ethtool', '-L', interface, 'combined', '1']),
                RemoteHost(address).run(['export', 'IRQBALANCE_BANNED_CPULIST=0-6', ';', 'irqbalance'])])
            tasks.append(task)
Parallel(tasks).start(wait=True)
