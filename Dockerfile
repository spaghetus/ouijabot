FROM rust
WORKDIR /src
COPY . .
RUN cargo install --path .
CMD ["askouija", "--dict=words.txt"]