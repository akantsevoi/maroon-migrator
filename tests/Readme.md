# High-level test scenarious

## 1
- Client 1 connects to gw 1
- Gw 1 requests some range (for 16 elements).
- Client 2 connects to gw 2.
- Gw 2 requests some range (for 16 elements).
- Client 2 subscribes to updates.
- Client 1 pushes 5 messages.
- Client 2 sees 5 messages (“push”).

