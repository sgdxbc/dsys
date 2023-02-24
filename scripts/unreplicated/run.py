#!/usr/bin/env python3
from asyncio import gather
from subprocess import PIPE
import sys

sys.path.append("scripts")
from lib import load_instances


async def evaluate(client_count):
    replica_instance = None
    client_instances = []
    for instance in load_instances():
        if instance.role == "replica":
            replica_instance = replica_instance or instance
        if instance.role == "client" and len(client_instances) < client_count:
            client_instances.append(instance)
    assert replica_instance
    assert len(client_instances) == client_count

    async def clean_up():
        await replica_instance.tmux_kill("unreplicated")
        await gather(*[instance.pkill("unreplicated") for instance in client_instances])

    print("clean up", file=sys.stderr)
    await clean_up()

    async def evaluate_internal():
        print("launch replica", file=sys.stderr)
        await replica_instance.tmux("unreplicated", "./unreplicated replica")

        print("launch clients", file=sys.stderr)
        clients = [
            await instance.start(
                f"./unreplicated client {replica_instance.ip}", stdout=PIPE, stderr=PIPE
            )
            for instance in client_instances
        ]

        print("wait clients", end="", flush=True, file=sys.stderr)
        for client in clients:
            await client.wait()
            print(".", end="", flush=True)
        print()

        print("interrupt replica", file=sys.stderr)
        # if replica crashed before this interruption, this becomes a nop and leave
        # the tmux session alive for debugging
        await replica_instance.pkill("-INT unreplicated")

        count = 0
        output_lantecy = True
        for client in clients:
            out, err = await client.communicate()
            if client.returncode != 0:
                count = None
                print(err.decode())
            if count is None:
                break

            [client_count, latency, *stats] = out.decode().splitlines()
            count += float(client_count)

            if output_lantecy:
                print(latency)
                output_lantecy = False
            # if stats:
            #     print('\n'.join(stats))
        if count is not None:
            print(count)

    try:
        await evaluate_internal()
    except:
        print("clean up on exception")
        await clean_up()
        raise


if __name__ == "__main__":
    from lib import args
    from asyncio import run

    if args(1) == "test":
        run(evaluate(int(args(2, "1"))))
    else:
        for client_count in [1, 2, 5, 10, 20, 50, 100, 200, 300, 400, 500]:
            print(client_count)
            run(evaluate(client_count))

else:
    print("execute this script")
    exit(1)
