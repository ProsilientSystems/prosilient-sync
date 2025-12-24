## Admin
-------
* Approve devices by adding public key + expirable signature for authorization 
* Admin can accept initial connection request and set validity period
* Admin can send notifications to devices
* Admin can disable/block device

## Devices
-------
> Backup protocol must be decided incase connection is not reliable for websockets, fallback to http polling should exist.

> Can we send wifi configuration information? that may count as idenfiable information...

* Connects with server, authenticates with certificate and registers websocket

* Send logs(error, failure, etc.)
* Send Analytics/telemetry
* Send biosensor data
* Send scale data
* Send diagnostic information (last biosensor connection, last scale connection, last user interaction, number of reboots, number of power failures)

```[[Pushed]]```
* Receive notifications from the server (special surveys)
* Receive updated survey questions
* Receive wifi configuration commands (delete all, add new)
* Receive ble configurations (add biosensor, add scale, remove biosensor)