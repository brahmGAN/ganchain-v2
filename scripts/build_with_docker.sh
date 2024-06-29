# for support wild range os

docker run --rm -it -w /project/gpu \
-v $(pwd):/project/gpu \
paritytech/ci-linux:production cargo build --profile production
