# Event Model

Event Model is how a server application handle an io event, parse a message,
process, and send back a response if necessary.

This library provide 3 model:

- Synchronous
- Bidirectional
- Multiplex

## `Synchronous`

The `Synchronous` model, also known as Request Response model, is an event
model where single message is processed and produced another message as
response, one at a time.

In `Synchronous` model, the client is the one who initiate the message cycle
(Request), then the server respond back with another message (Response), but
not vice versa.

The `Synchronous` model is used with the `http` protocol. This model is very
simple where one event is processed sequentially. The downside is that, this
model cannot handle multiple message at a time.

## `Bidirectional`

The `Bidirectional` model, is an event model where either client and server can
send and receive message at any time.

The `Bidirectional` model is used with the `WebSocket` and `PostreSQL`
protocol. This model is `Asynchronous`, which means multiple message can be
processed at a time. The downside is that if a message expect a corresponding
response message, the application should be the one figuring out whether the
received message is a server message or a response message.

## `Multiplex`

The `Multiplex` model is combination of previous models, where a request
message expect a response message, and multiple messages can be processed at
the same time.

# Protocol

Protocol is a ruleset that describe how a message is transmitted and received
between client and server.

Each protocol have their own event model.

- `http/1.1` uses `Synchronous` model
- `http/2.0` uses `Multiplex` model
- `WebSocket` uses `Bidirectional` model
- `PostreSQL` uses `Bidirectional` model
- `LSP` uses `Bidirectional` model

