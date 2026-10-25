using UnityEngine;

public sealed class HostProbe : MonoBehaviour
{
    [SerializeField] private int displacement = 0;
    [SerializeField] private Transform target = null!;
    private void Awake() { target.position = new Vector3(displacement, 0, 0); }
    private void Update()
    {
        if (Input.GetKeyDown(KeyCode.W)) target.position += new Vector3(Time.deltaTime, 1, 0);
    }
}
