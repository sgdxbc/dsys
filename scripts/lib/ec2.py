from lib import Instance, Spec


def to_spec(instance_type):
    if instance_type == "m5.4xlarge":
        return Spec(16, "ens5", 1 << 14, 1 << 10)

    raise ValueError(instance_type)


def launch(ec2, instance_type, count, params):
    return ec2.create_instances(
        InstanceType=instance_type,
        ImageId=params["image"],
        SubnetId=params["subnet"],
        KeyName=params["key"],
        MinCount=count,
        MaxCount=count,
        TagSpecifications=[
            {"ResourceType": "instance", "Tags": [{"Key": "dsys", "Value": "default"}]}
        ],
    )


def wait_running(instance, role):
    instance.wait_until_running()
    instance.reload()
    return Instance(role, instance.public_ip_address, instance.private_ip_address)


def terminate(ec2):
    instances = ec2.instances.filter(Filters=[{"Name": "tag:dsys", "Values": ["*"]}])
    instances_list = list(instances)
    instances.terminate()
    print("termination requested")

    for instance in instances_list:
        if instance.state["Name"] == "terminated":
            continue
        instance.wait_until_terminated()
        print(".", end="", flush=True)
    print()
