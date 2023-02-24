from lib import Instance, Spec


specs = {
    "c5.2xlarge": Spec(8, "ens5", 1 << 14, 1 << 10),
    "m5.4xlarge": Spec(16, "ens5", 1 << 14, 1 << 10),
    "c5.4xlarge": Spec(16, "ens5", 1 << 14, 1 << 10),
    "c5.12xlarge": Spec(48, "ens5", 1 << 14, 1 << 10),
}


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
    instances_list = list(
        instance for instance in instances if instance.state["Name"] != "terminated"
    )
    instances.page_size(300).terminate()
    print("termination requested")

    for instance in instances_list:
        instance.wait_until_terminated()
        print(".", end="", flush=True)
    print()
