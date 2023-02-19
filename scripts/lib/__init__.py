from asyncio import create_subprocess_shell, create_subprocess_exec
from sys import argv
from subprocess import DEVNULL


def args(i, default=None):
    return argv[i] if len(argv) > i else default


class Instance:
    def __init__(self, role, control_host, ip):
        self.role = role
        self.control_host = control_host
        self.ip = ip

    def store(self):
        return f"{self.role:12}{self.control_host:20}{self.ip}"

    def setup(self, spec):
        return setup(self.control_host, spec)

    def rsync(self, path):
        return rsync(self.control_host, path)

    def start(self, command, stdout=None, stderr=None):
        return remote_start(self.control_host, command, stdout=stdout, stderr=stderr)

    def pkill(self, name):
        return pkill(self.control_host, name)

    def tmux(self, session, command):
        return tmux(self.control_host, session, command)

    def tmux_interrupt(self, session):
        return tmux_interrupt(self.control_host, session)


def load_instances():
    instances = []
    with open("run-instances.txt") as f:
        for line in f:
            [role, control_host, ip] = line.split()
            instances.append(Instance(role, control_host, ip))
    return instances


def local_start(command):
    return create_subprocess_shell(command)


async def wait_process(process, check_return=0):
    process = await process
    await process.wait()
    if check_return is not None:
        assert process.returncode == check_return


def build(package):
    return wait_process(local_start(f"cargo build --release --package {package}"))


def rsync(address, path):
    return local_start(f"rsync --rsh='ssh -q' --update --times {path} {address}:")


def remote_start(address, command, stdout=None, stderr=None):
    return create_subprocess_exec(
        "ssh", "-q", address, command, stdout=stdout, stderr=stderr
    )


def pkill(address, name):
    return wait_process(remote_start(address, f"pkill {name}"), check_return=None)


def tmux(address, session, command):
    return wait_process(
        remote_start(address, f"tmux new-session -d -s {session} '{command} || read _'")
    )


def tmux_interrupt(address, session):
    return wait_process(remote_start(address, f"tmux send-key -t {session} C-c"))


class Spec:
    def __init__(self, core_count, interface, rx_ring, tx_ring):
        self.core_count = core_count
        self.interface = interface
        self.rx_ring = rx_ring
        self.tx_ring = tx_ring


def setup(address, spec):
    return remote_start(
        address,
        # interface queue count, rx/tx ring buffer size
        f"sudo ethtool -L {spec.interface} combined 1 && "
        f"sudo ethtool -G {spec.interface} rx {spec.rx_ring} tx {spec.tx_ring} && "
        # disable hyperthreading
        f"for i in $(seq {spec.core_count // 2} {spec.core_count - 1}); do "
        "echo 0 | sudo tee /sys/devices/system/cpu/cpu$i/online; done && "
        # bind IRQ to the last core
        "sudo service irqbalance stop && "
        f"IRQBALANCE_BANNED_CPULIST=0-{spec.core_count // 2 - 2} sudo -E irqbalance --oneshot && "
        # IGMPv2 required by AWS VPC's multicast
        f"sudo sysctl net.ipv4.conf.{spec.interface}.force_igmp_version=2",
        stdout=DEVNULL,
    )
