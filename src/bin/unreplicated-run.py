#!/usr/bin/env python3
from pyrem.host import RemoteHost
from pyrem.task import Parallel


with open('addresses.txt') as addresses:
    replica_address = None
    client_addresses = []
    for line in addresses:
        [role, address] = line.split()
        if role == 'replica':
            replica_address = replica_address or address
        if role == 'client':
            client_addresses.append(address)

assert replica_address
replica = RemoteHost(replica_address).run(['./unreplicated-replica', replica_address], quiet=True)
replica.start()
Parallel([
    RemoteHost(client).run(['./unreplicated-client', client, replica_address], quiet=True) 
    for client in client_addresses]
).start(wait=True)
replica.stop()

count = 0
output_latency = True
for client in client_addresses:
    result = RemoteHost(client).run(['cat', 'result.txt'], return_output=True).start(wait=True)['stdout'].decode()
    if not result:
        print('(Result incomplete)')
        exit(1)
    [client_count, latency] = result.splitlines()
    if output_latency:
        print(latency)
        output_latency = False
    count += int(client_count)
print(count)
