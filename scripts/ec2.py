#!/usr/bin/env python3
import sys
import boto3
from pyrem.host import RemoteHost
from pyrem.task import Parallel


def params_replica(i):
    assert i < 254
    ip = f'172.31.1.{i + 1}'
    return {
        'SubnetId': 'subnet-008ff4a08e63a0794',
        'PrivateIpAddress': ip,
        'ImageId': 'ami-0bc44b8dc7cae9c34',
        'InstanceType': 'm5.2xlarge',
        'KeyName': 'Ephemeral',
    }


def params_client(i):
    assert i < 1024
    ip = f'172.31.{2 + i // 254}.{1 + i % 254}'
    return {
        'SubnetId': 'subnet-008ff4a08e63a0794',
        'PrivateIpAddress': ip,
        'ImageId': 'ami-0bc44b8dc7cae9c34',
        'InstanceType': 't3.micro',
        'KeyName': 'Ephemeral',
    }


def params_seq(i):
    assert i == 0
    ip = '172.31.0.4'
    return {
        'SubnetId': 'subnet-008ff4a08e63a0794',
        'PrivateIpAddress': ip,
        'ImageId': 'ami-0bc44b8dc7cae9c34',
        'InstanceType': 'c6i.16xlarge',
        'KeyName': 'Ephemeral',        
    }


params = {
    'replica': params_replica,
    'client': params_client,
}
ec2 = boto3.resource('ec2', region_name='ap-east-1')

if sys.argv[1:2] == ['launch']:
    instances = []
    addresses = ''
    for arg in sys.argv[2:]:
        [role, count] = arg.split('=')
        for i in range(int(count)):
            instance = ec2.create_instances(
                **params[role](i), 
                MinCount=1, MaxCount=1, 
                TagSpecifications=[{'ResourceType': 'instance', 'Tags': [{'Key': 'role', 'Value': role}]}],
            )[0]
            instances.append(instance)
            addresses += f'{role:12}{instance.private_ip_address}\n'
    for instance in instances:
        instance.wait_until_running()
        print('.', end='', flush=True)
    print()
    with open('addresses.txt', 'w') as addresses_file:
        addresses_file.write(addresses)
elif sys.argv[1:2] == ['terminate']:
    instances = ec2.instances.filter(Filters=[
        {'Name': 'instance-state-name', 'Values': ['running']},  # other states?
        {'Name': 'tag:role', 'Values': ['*']}])
    instances.terminate()
    # for instance in instances:
    #     instance.wait_until_terminated()
    #     print('.', end='', flush=True)
    # print()
    with open('addresses.txt', 'w') as addresses_file:
        pass  # clear it
else:
    print(f'Usage: {sys.argv[0]} launch|terminate')
