#!/bin/bash

# Initialize variables
client_port="8000"
registry_path="registry-200.txt"
latency_limit="400"

build=false

function usage() {
    echo "Usage: $0 -p <port> -r <replicas> -l <latency>"
    echo "  -p  Set the client API port (default: $client_port)"
    echo "  -r  Registry file location (default: $registry_path)"
    echo "  -l  Set the latency limit in milliseconds (default: $latency_limit)"
    echo "  -b  Build images (default: $build)"
    exit 1
}


# Parse options
while getopts "p:r:l:bh" opt; do
    case $opt in
        p) client_port=$OPTARG;;
        r) registry_path=$OPTARG;;
        l) latency_limit=$OPTARG;;
        b) build=true;;
        *) usage;;
    esac
done

# Output the input options for verification
echo "Client Port: $client_port"
echo "Registry location: $registry_path"
echo "Latency Limit: $latency_limit"

if [ "$build" = true ]; then
    echo "Building docker images in $(pwd)"
    docker build -t dato-validator -f Dockerfile.validator --load .
    docker build -t dato-client -f Dockerfile.client --load .
    
    exit 0
fi

echo "Creating dato-net network"
docker network create -d bridge dato-net

while IFS=',' read -r index privkey pubkey
do
    instance="dato-validator-$index"
    echo "Starting $instance"

    docker run -d --network dato-net --name $instance -e RUST_LOG=debug dato-validator run --secret-key $privkey --port 8222

    rand_latency=$(( ( RANDOM % $latency_limit ) + 1 ))

    # Add latency to the validator instance
    docker exec $instance tc qdisc add dev eth0 root netem delay {$latency_limit}ms
done < "$registry_path"
exit 0


echo "Starting dato-client"
docker run -d --network dato-net --name dato-client -p $client_port:$client_port dato-client --registry-file $registry_path --api-port $client_port

# Iterate over the lines in the registry file