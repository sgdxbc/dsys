#!/usr/bin/env python3
from tempfile import TemporaryDirectory
from pyrem.host import RemoteHost
from pyrem.task import Parallel


def run(client_count):
    with open('addresses.txt') as addresses:
        replica_address = None
        client_addresses = []
        for line in addresses:
            [role, address] = line.split()
            if role == 'replica':
                replica_address = replica_address or address
            if role == 'client' and len(client_addresses) < client_count:
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
    with TemporaryDirectory() as workspace:
        Parallel([
            RemoteHost(client).get_file('result.txt', f'{workspace}/{client}.txt', quiet=True) 
            for client in client_addresses]
        ).start(wait=True)
        for client in client_addresses:
            with open(f'{workspace}/{client}.txt') as result:
                result = result.read()
                if not result:
                    raise Exception('Result incomplete')
                [client_count, latency] = result.splitlines()
                if output_latency:
                    print(latency)
                    output_latency = False
                count += int(client_count)
        print(count / 10)


# for client_count in [1, 2, 5, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100]:
for client_count in [100]:
    print(client_count)
    run(client_count)
