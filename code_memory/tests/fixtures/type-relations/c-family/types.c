typedef struct Payload {
    int value;
} Payload;

typedef struct Holder {
    Payload current;
} Holder;

Payload transform(Payload input) {
    Payload transient = input;
    return transient;
}

int main(void) {
    Payload local = {1};
    Holder holder = {local};
    return transform(holder.current).value;
}
