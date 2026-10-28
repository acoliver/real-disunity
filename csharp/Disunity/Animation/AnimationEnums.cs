namespace UnityEngine;

public enum AvatarTarget
{
    Root = 0, Body = 1, LeftFoot = 2, RightFoot = 3, LeftHand = 4, RightHand = 5,
}

public enum AvatarIKGoal
{
    LeftFoot = 0, RightFoot = 1, LeftHand = 2, RightHand = 3,
}

public enum AvatarIKHint
{
    LeftKnee = 0, RightKnee = 1, LeftElbow = 2, RightElbow = 3,
}

public enum AnimatorControllerParameterType
{
    Float = 1, Int = 3, Bool = 4, Trigger = 9,
}

public enum AnimatorRecorderMode
{
    Offline = 0, Playback = 1, Record = 2,
}

public enum HumanBodyBones
{
    Hips = 0, LeftUpperLeg = 1, RightUpperLeg = 2, LeftLowerLeg = 3, RightLowerLeg = 4, LeftFoot = 5, RightFoot = 6,
    Spine = 7, Chest = 8, UpperChest = 54, Neck = 9, Head = 10, LeftShoulder = 11, RightShoulder = 12,
    LeftUpperArm = 13, RightUpperArm = 14, LeftLowerArm = 15, RightLowerArm = 16, LeftHand = 17, RightHand = 18,
    LeftToes = 19, RightToes = 20, LeftEye = 21, RightEye = 22, Jaw = 23,
    LeftThumbProximal = 24, LeftThumbIntermediate = 25, LeftThumbDistal = 26,
    LeftIndexProximal = 27, LeftIndexIntermediate = 28, LeftIndexDistal = 29,
    LeftMiddleProximal = 30, LeftMiddleIntermediate = 31, LeftMiddleDistal = 32,
    LeftRingProximal = 33, LeftRingIntermediate = 34, LeftRingDistal = 35,
    LeftLittleProximal = 36, LeftLittleIntermediate = 37, LeftLittleDistal = 38,
    RightThumbProximal = 39, RightThumbIntermediate = 40, RightThumbDistal = 41,
    RightIndexProximal = 42, RightIndexIntermediate = 43, RightIndexDistal = 44,
    RightMiddleProximal = 45, RightMiddleIntermediate = 46, RightMiddleDistal = 47,
    RightRingProximal = 48, RightRingIntermediate = 49, RightRingDistal = 50,
    RightLittleProximal = 51, RightLittleIntermediate = 52, RightLittleDistal = 53, LastBone = 55,
}
