#!/usr/bin/env python3
from asyncio import create_subprocess_exec, run, gather, sleep
from subprocess import PIPE
from sys import stderr

async def remote(address, args, stdout=None, stderr=None):
    return await create_subprocess_exec(
        'ssh', '-q', address, *args, stdout=stdout, stderr=stderr)

async def remote_sync(address, args):
    p = await remote(address, args)
    await p.wait()
    return p.returncode

async def eval(client_count):
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
    assert len(client_addresses) == client_count

    print('clean up', file=stderr)
    await remote_sync(replica_address, ['pkill', 'unreplicated'])
    await gather(*[
        remote_sync(client_address, ['pkill', 'unreplicated'])
        for client_address in client_addresses])

    print('launch replica', file=stderr)
    replica = await remote(
        replica_address, 
        ['tmux', 'new-session', '-d', '-s', 'unreplicated', './unreplicated-replica', replica_address])
    await replica.wait()
    await sleep(1)

    print('launch clients', file=stderr)
    clients = [
        await remote(
            client_address, 
            ['./unreplicated-client', client_address, replica_address], 
            stdout=PIPE, stderr=PIPE)
        for client_address in client_addresses]

    print('wait clients', end='', flush=True, file=stderr)
    for client in clients:
        await client.wait()
        print('.', end='', flush=True)
    print()

    # capture output before interrupt?
    print('interrupt replica', file=stderr)
    replica = await remote(replica_address, ['tmux', 'send-key', '-t', 'unreplicated', 'C-c'])
    await replica.wait()

    count = 0
    output_lantecy = True
    for client in clients:
        out, err = await client.communicate()
        if client.returncode != 0:
            count = None
            print(err.decode())
        [client_count, latency] = out.decode().splitlines()
        if count is not None:
            count += int(client_count)
        if output_lantecy:
            print(latency)
            output_lantecy = False
    print(count / 10)

    print('clean up', file=stderr)
    await remote_sync(replica_address, ['pkill', 'unreplicated'])
    await gather(*[
        remote_sync(client_address, ['pkill', 'unreplicated'])
        for client_address in client_addresses])

for client_count in [1, 2, 5, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100]:
# for client_count in [100]:
    print(client_count)
    run(eval(client_count))
