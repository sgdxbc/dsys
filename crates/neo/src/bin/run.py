#!/usr/bin/env python3
from asyncio import create_subprocess_exec, gather, sleep
from subprocess import PIPE
from sys import stderr

async def remote(address, args, stdout=None, stderr=None):
    return await create_subprocess_exec(
        'ssh', '-q', address, *args, stdout=stdout, stderr=stderr)

async def remote_sync(address, args):
    p = await remote(address, args)
    await p.wait()
    return p.returncode

async def evaluate(replica_count, client_count, crypto):
    with open('addresses.txt') as addresses:
        seq_address = None
        replica_addresses, client_addresses = [], []
        for line in addresses:
            [role, public_address, address] = line.split()
            if role == 'replica' and len(replica_addresses) < replica_count:
                replica_addresses.append((public_address, None))
            if role == 'client' and len(client_addresses) < client_count:
                client_addresses.append((public_address, None))
            if role == 'seq':
                seq_address = seq_address or (public_address, address)
    assert seq_address is not None
    assert len(replica_addresses) == replica_count
    assert len(client_addresses) == client_count
    f = (replica_count - 1) // 3

    print('clean up', file=stderr)
    await gather(*[
        remote_sync(address[0], ['pkill', 'neo'])
        for address in client_addresses + replica_addresses + [seq_address]])

    print('launch sequencer', file=stderr)
    await remote_sync(
        seq_address[0], [
            'tmux', 'new-session', '-d', '-s', 'neo', 
            './neo-seq', 
                '--multicast', '239.255.1.1', 
                '--replica-count', str(replica_count),
                '--crypto', crypto])

    print('launch replicas', file=stderr)
    await gather(*[remote_sync(
        replica_address[0], [
            'tmux', 'new-session', '-d', '-s', 'neo', 
            './neo-replica', 
                '--id', str(i), 
                '--multicast', '239.255.1.1', 
                '-f', str(f), 
                '--tx-count', '5',
                '--crypto', crypto])
        for i, replica_address in enumerate(replica_addresses)])

    print('wait replicas to join multicast group', file=stderr)
    await sleep(10)

    print('launch clients', file=stderr)
    clients = [
        await remote(
            client_address[0], [
                './neo-client', '--seq-ip', seq_address[1], '-f', str(f)], 
            stdout=PIPE, stderr=PIPE)
        for client_address in client_addresses]

    print('wait clients', end='', flush=True, file=stderr)
    for client in clients:
        await client.wait()
        print('.', end='', flush=True)
    print()

    # capture output before interrupt?
    print('interrupt sequencer and replicas', file=stderr)
    await gather(*[
        remote_sync(address[0], ['tmux', 'send-key', '-t', 'neo', 'C-c'])
        for address in replica_addresses + [seq_address]])
    
    count = 0
    output_lantecy = True
    for client in clients:
        out, err = await client.communicate()
        if client.returncode != 0:
            count = None
            print(err.decode(), file=stderr)
        if count is None:
            break
        [client_count, latency] = out.decode().splitlines()
        count += int(client_count)
        if output_lantecy:
            print(latency)
            output_lantecy = False
    if count is not None:
        print(count / 10)

    print('clean up', file=stderr)
    await gather(*[
        remote_sync(address[0], ['pkill', 'neo'])
        for address in client_addresses + replica_addresses + [seq_address]])
    return count

if __name__ == '__main__':
    from sys import argv
    from asyncio import run
    if argv[1:2] == ['test']:
        run(evaluate(1, 200, argv[2]))
    else:
        client_count = 80
        wait = False
        for replica_count in [1, 4, 7, 10, 13, 16]:
            if wait:
                print('wait to prevent interference between runs', file=stderr)
                run(sleep(20))
            else:
                wait = True
            print(replica_count, client_count)
            retry = True
            while retry:
                retry = run(evaluate(replica_count, client_count, argv[1])) is None
