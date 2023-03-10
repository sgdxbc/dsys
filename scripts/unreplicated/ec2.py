#!/usr/bin/env python3
import runpy
import pathlib
import sys
import asyncio
import boto3

sys.path.append("scripts")
from lib import args, wait_process
from lib.ec2 import launch, wait_running, terminate, specs

client_type = "t3.micro"
replica_type = "c5.2xlarge"


def launch_action(ec2, client_count, params):
    print(f"launch {client_count} client(s)")
    client_instances = launch(ec2, client_type, client_count, params)
    print("launch one replica")
    replica_instances = launch(ec2, replica_type, 1, params)

    instances = []
    print("wait for replica running")
    for instance in replica_instances:
        instances.append(wait_running(instance, "replica"))

    async def task():
        print("set up replica")
        await asyncio.sleep(3)
        for instance in instances:
            await wait_process(instance.setup(specs[replica_type]))

    asyncio.run(task())

    print("wait for clients running")
    for instance in client_instances:
        instances.append(wait_running(instance, "client"))
        print(".", end="", flush=True)
    print()

    return instances


def main():
    params = runpy.run_path("run-ec2params.py")
    if "profile" in params:
        boto3.setup_default_session(profile_name=params["profile"])
    ec2 = boto3.resource("ec2", region_name=params["region"])
    out = pathlib.Path("run-instances.txt")

    if args(1) == "launch":
        client_count = 1
        client_count = int(args(2, "1"))
        assert not out.exists()
        try:
            instances = launch_action(ec2, client_count, params)
        except:
            print("terminate on exception")
            terminate(ec2)
            raise

        out.write_text("\n".join(instance.store() for instance in instances))
        exit()

    if args(1) == "terminate":
        terminate(ec2)
        out.unlink(missing_ok=True)
        exit()

    print(f"{args(0)} launch|terminate")


if __name__ == "__main__":
    main()
