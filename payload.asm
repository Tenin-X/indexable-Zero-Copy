use16
org 0x1000

start:
    mov ax, 0x1234
    mov bx, 0x5678
    mov cx, ax
    add cx, bx
    out 0x10, al     ; I/O port write to cause VM exit
    jmp start
