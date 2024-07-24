# Start from the Ubuntu base image
FROM ubuntu:latest

# Install necessary packages
RUN apt-get update && apt-get install -y wget && apt-get install -y curl

# Set the working directory
WORKDIR /app

# Download the binary file from GitHub
RUN wget -O binary-file https://github.com/brahmGAN/ganchain-v2/releases/download/v1.0.0/gpu

# Give execute permission to the binary
RUN chmod +x binary-file

#Get name from env
ENV NAME="GANValidator"

# Command to run the binary
CMD ./binary-file --base-path chaindata/GanValidator --chain gpu --port 30333 --rpc-port 9933 --validator --name ${NAME}  --bootnodes /ip4/35.154.205.124/tcp/30335/ws/p2p/12D3KooWAhHneKawZeMo1Y4zwMcBb1Tc4sJG5KBqyaE25ou5GfKK