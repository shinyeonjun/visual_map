#ifndef VISUAL_MAP_TYPES_H
#define VISUAL_MAP_TYPES_H

typedef struct User {
    const char *id;
} User;

typedef struct Box {
    User value;
} Box;

static inline const char *box_id(const Box *box) {
    return box->value.id;
}

#endif
