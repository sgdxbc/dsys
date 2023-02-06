#!/usr/bin/env python3
from tempfile import TemporaryDirectory
from pyrem.host import RemoteHost
from pyrem.task import Parallel


def run(replica_count, client_count):
    with open("addresses.txt") as addresses:
        replica_addresses, client_addresses, seq_address = [], [], None
        for line in addresses:
            [role, address] = line.split()
            if role == "seq":
                assert not seq_address
                seq_address = address
            if role == "replica" and len(replica_addresses) < replica_count:
                replica_addresses.append(address)
            if role == "client" and len(client_addresses) < client_count:
                client_addresses.append(address)

    assert seq_address
    seq_command = ["./neo-seq", "--addr", seq_address]
    for replica in replica_addresses:
        seq_command.extend(["--broadcast", replica])
    seq = RemoteHost(seq_address).run(seq_command, quiet=True)
    seq.start()
    replicas = Parallel([
        RemoteHost(replica).run(["./neo-replica", str(i), replica], quiet=True)
        for i, replica in enumerate(replica_addresses)])
    replicas.start()

    f = (len(replica_addresses) - 1) // 3
    Parallel([
        RemoteHost(client).run(["./neo-client", str(f), client, seq_address], quiet=True)
        for client in client_addresses]
    ).start(wait=True)

    seq.stop()
    replicas.stop()

    count = 0
    output_latency = True
    with TemporaryDirectory() as workspace:
        Parallel([
            RemoteHost(client).get_file("result.txt", f"{workspace}/{client}.txt", quiet=True)
            for client in client_addresses]
        ).start(wait=True)
        for client in client_addresses:
            with open(f"{workspace}/{client}.txt") as result:
                result = result.read()
                if not result:
                    raise Exception("Result incomplete")
                [client_count, latency] = result.splitlines()
                if output_latency:
                    print(latency)
                    output_latency = False
                count += int(client_count)
        print(count / 10)


# for client_count in [1, 2, 5, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100]:
for replica_count, client_count in [(4, 10)]:
    print(replica_count, client_count)
    run(replica_count, client_count)
