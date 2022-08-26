# Impl Details for IO Context

# Actions

-   TcpListerner::incoming / Incoming::next (iterator over tcp accept)
-   TcpListener::bind (resolved used module local context)
-   TcpListener::accept (waits for new socket connection --> register waiting state, adds new mngt entry when success)
-   TcpStream::connect (create wait entry, declare network intent, resolved after other side confirmes)
-   TcpStream::read (wait intent / or direct resolve from buffer)
-   TcpStream::write (into intent buffer)
-   UdpSocket::bind UdpSocket::connect ( instant resolve)
-   UdpSocket::read
-   UdpSocket::write
-   dns_resolve (Intent with wakeup)
