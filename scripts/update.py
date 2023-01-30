#!/usr/bin/env python3
from pyrem.host import LocalHost, RemoteHost
from pyrem.task import Parallel

LocalHost().run(['cargo', 'build', '--release'], require_success=True).start(wait=True)

with open('addresses.txt') as addresses:
    tasks = []
    for line in addresses:
        [role, address] = line.split()
        file_name = f'unreplicated-{role}'
        task = RemoteHost(address).send_file(f'target/release/{file_name}', file_name, quiet=True)
        tasks.append(task)
Parallel(tasks).start(wait=True)
