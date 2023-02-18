#!/usr/bin/env python3
import sys
import runpy
import concurrent.futures
import pathlib
import boto3
import botocore.config
import tqdm


def main():
    config = {
        'profile': 'default',
        'region': 'ap-east-1',
        'image_id': 'ami-0bc44b8dc7cae9c34',  # ubuntu 22.04
    }
    config.update(runpy.run_path('run_ec2_config.py'))

    boto3.setup_default_session(profile_name=config['profile'])
    ec2 = boto3.resource(
        'ec2', 
        region_name=config['region'], 
        config=botocore.config.Config(retries={'max_attempts': 1024, 'mode': 'adaptive'}))

    if sys.argv[1:2] == ['launch']:
        assert not pathlib.Path('run_addr.txt').exists()

        preset = runpy.run_path(sys.argv[2])
        try:
            instances = launch(preset, sys.argv[3:], ec2, config)
        except:
            terminate(ec2)
            raise

        addresses = ''
        for role, instance in instances:
            instance.wait_until_running()
            instance.reload()
            addresses += f'{role:12}{instance.public_ip_address:20}{instance.private_ip_address}\n'
            print('.', end='', flush=True)
        print()
        with open('run_addr.txt', 'w') as addresses_file:
            addresses_file.write(addresses)
        exit()

    if sys.argv[1:2] == ['terminate']:
        terminate(ec2)
        pathlib.Path('run_addr.txt').unlink(missing_ok=True)
        exit()

    print(f'Usage: {sys.argv[0]} launch|terminate')


def launch(preset, args, ec2, config):
    plans = []
    for arg in args:
        [role, count] = arg.split('=')
        plans += [(role, i) for i in range(int(count))]
    instances = []
    # a little bit faster than purely single threaded (about 2x)
    # this is limited by AWS's bucket refill rate (2 requests/second), so no more improvement
    # if still cannot find a way to launch multiple instances in one request
    # leverage boto3's client-side throttle (adaptive retry mode)
    with concurrent.futures.ThreadPoolExecutor() as executor, tqdm.tqdm(total=len(plans)) as pbar:
        for role, i in plans:
            def task(role, i):
                instance = ec2.create_instances(
                    **preset[f'launch_{role}'](i, config),
                    MinCount=1, MaxCount=1, 
                    TagSpecifications=[{
                        'ResourceType': 'instance', 
                        'Tags': [{'Key': 'dsys-role', 'Value': role}]}],
                )[0]
                pbar.update()
                return instance
            instances.append((role, executor.submit(task, role, i)))
            # addresses += f'{role:12}{instance.private_ip_address}\n'
        return [(role, instance.result()) for role, instance in instances]


def terminate(ec2):
    instances = ec2.instances.filter(Filters=[
        {'Name': 'instance-state-name', 'Values': ['pending', 'running']},  # other states?
        {'Name': 'tag:dsys-role', 'Values': ['*']}])
    instances_list = list(instances)
    instances.terminate()
    print('termination requested')
    for instance in instances_list:
        instance.wait_until_terminated()
        print('.', end='', flush=True)
    print()
    print('terminated')


if __name__ == '__main__':
    main()
