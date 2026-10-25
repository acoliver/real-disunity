using UnityEngine;

public sealed class Example : MonoBehaviour
{
    public GameObject Duplicate(GameObject template)
    {
        var clone = Instantiate(template, transform);
        clone.transform.position = transform.position;
        return clone;
    }
    private void Update() => _ = Time.deltaTime;
}
