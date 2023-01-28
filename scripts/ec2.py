import sys
import boto3


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


params = {
    'replica': params_replica,
    'client': params_client,
}
ec2 = boto3.resource('ec2', region_name='ap-east-1')

if sys.argv[1:2] == ['create']:
    instances = []
    for arg in sys.argv[2:]:
        [role, count] = arg.split('=')
        for i in range(int(count)):
            instance = ec2.create_instances(
                **params[role](i), 
                MinCount=1, MaxCount=1, 
                TagSpecifications=[{'ResourceType': 'instance', 'Tags': [{'Key': 'role', 'Value': 'replica'}]}],
            )[0]
            instances.append(instance)
    for instance in instances:
        instance.wait_until_running()
        print('.', end='', flush=True)
    print()
elif sys.argv[1:2] == ['terminate']:
    instances = ec2.instances.filter(Filters=[
        {'Name': 'instance-state-name', 'Values': ['running']},  # other states?
        {'Name': 'tag:role', 'Values': ['*']}])
    instances.terminate()
    for instance in instances:
        instance.wait_until_terminated()
        print('.', end='', flush=True)
    print()
else:
    print(f'Usage: {sys.argv[0]} create|terminate')
