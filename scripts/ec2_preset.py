spec_replica = {
    'interface': 'ens5',
    'rx': 16384,
    'tx': 1024,
    '#core': 16,
}

def launch_replica(i, config):
    assert i < 254
    ip = f'172.31.1.{i + 1}'
    return {
        'SubnetId': config['subnet'],
        'PrivateIpAddress': ip,
        'ImageId': config['image_id'],
        'InstanceType': 'm5.4xlarge',
        'KeyName': 'Ephemeral',
    }


def launch_client(i, config):
    assert i < 1024
    ip = f'172.31.{2 + i // 254}.{1 + i % 254}'
    return {
        'SubnetId': config['subnet'],
        'PrivateIpAddress': ip,
        'ImageId': config['image_id'],
        'InstanceType': 't3.micro',
        'KeyName': 'Ephemeral',
    }
