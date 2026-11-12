#include <compartment.h>
#include <debug.hh>
#include <fail-simulator-on-error.h>

using Debug = ConditionalDebug<true, "{{name}}">;

int __cheri_compartment("hello") say_hello()
{
	Debug::log("Hello world");
	return 0;
}
