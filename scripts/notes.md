The Organization of Scripts

Scripts are categorized into three parts.
The *library* that is independent to both user and protocol, the *parameters* 
that are independent to protocol but dependent to user, and the *executables* 
that are independent to user but dependent to protocol.

The typical examples of parameters are sensitive user configuration, e.g., the
EC2 region and subnet to launch instances, the name of key pair to access the 
instances, and the IPs of the instances. 
The parameters should be excluded from version control by the `/run*` rule.

The executables contain dedicated logic for evaluating each protocol. 
For example, the `unreplicated/update.py` executable builds root package and 
uploads `unreplicated` binary to the (single) replica and all clients. 
The reusable logic shared by executables are extracted into library and kept in
`lib` directory. 
Both executables and library read parameters for their various needs.

As of current development status most evaluation are conducted with AWS EC2
instances. 
Part of the scripts are EC2-dedicated like launching and terminating instances,
while the other scripts are generally appliable like setting up servers and 
uploading binaries. 
The EC2 scripts are kept in `ec2.py` files distributed among three parts.


`run-instances.txt`

This is the essential parameter as it defines how to access instances from 
control machine and how instances communicate to each other. 
The text format is derived from specpaxos. 
Every line defines an instance with three words: its role, its control 
hostname/IP and its private IP. 
For example, the parameter for the cluster of NUS System Lab may be:

```
replica     nsl-node1.d1    10.0.0.1
replica     nsl-node2.d1    10.0.0.2
...
```

There's no port in the private IP because some protocols use multiple ports.


EC2 notes

Current scripts launch EC2 instances with random (i.e., DHCP-assigned) private
IPs. 
This is a trade-off for launching performance. 
If specifying private IP during launching, only one instance can be launched per
request, and AWS effectively capped request rate to 2/second. 
All protocols I can think about do not require special IP patterns (i.e. it is 
possible to pass all IPs a node cares about to the node through e.g. command 
line).

According to AWS document launching/terminating instances seems to have similar
latency and rate limitation compare to starting/stopping instances. 
So the scripts implement management for one-shot instances to reduce 
complexity. 
For example, the `setup` method assumes freshly launched instances and there's 
no method to revert the setup.
Although currently all setup can be reverted by a reboot.

The scripts make heavy use of SSH. 
To work around various login issues caused by launching and terminating 
instances, add this to SSH configuration

```
Host * !github.com
    StrictHostKeyChecking no
    UserKnownHostsFile=/dev/null
    IdentityFile ~/?.pem
```
