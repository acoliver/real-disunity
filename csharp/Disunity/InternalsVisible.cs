using System.Runtime.CompilerServices;

// The managed tests read engine state that a renderer reads and the game cannot, such as a GraphicsBuffer's contents.
[assembly: InternalsVisibleTo("Disunity.Tests")]
