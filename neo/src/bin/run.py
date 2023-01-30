#!/usr/bin/env python3
from tempfile import TemporaryDirectory
from pyrem.host import RemoteHost
from pyrem.task import Parallel


def run(client_count):
    with open('addresses.txt') as addresses:
        replica_addresses, client_addresses, seq_address = [], [], None
        for line in addresses:
            [role, address] = line.split()
            if role == 'seq':
                assert not seq_address
                seq_address = address
            if role == 'replica':
                replica_addresses.append(replica_address)
            if role == 'client' and len(client_addresses) < client_count:
                client_addresses.append(address)

    assert seq_address
    seq_command = ['./neo_seq']
    for replica in replicas:
        seq_command.extend(['--broadcast', replica])
    seq = RemoteHost(seq_address).run(seq_command, quite=True)
    replicas = Parallel([
        RemoteHost(replica).run(['./neo-replica', str(i), replica_address], quiet=True)
        for i, replica in enumerate(replica_addresses)]
    ).start()
    replicas.start()

    f = len(replica_addresses - 1) // 3
    Parallel([
        RemoteHost(client).run(['./unreplicated-client', str(f), client, seq_address], quiet=True) 
        for client in client_addresses]
    ).start(wait=True)

    seq.stop()
    replicas.stop()

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
for client_count in [10]:
    print(client_count)
    run(client_count)
